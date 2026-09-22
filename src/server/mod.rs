use crate::common::config::Config;
use crate::common::{start_config_watcher, RateLimitConfigFile, RateLimitConfigManager};
use crate::tasks::ScheduledTasks;
use axum::{http::StatusCode, middleware, middleware::Next, response::IntoResponse, routing::get, Extension, Router};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use synapse_common::current_timestamp_millis;
use synapse_services::worker::topology_validator::{
    current_instance_worker_type, global_maintenance_owner, should_run_global_maintenance,
};
use synapse_web::middleware::{
    check_cors_security, log_cors_security_report, set_bind_address, set_config_allowed_origins,
    set_trust_forwarded_headers, validate_bind_address_for_dev_mode,
};
use synapse_web::routes::telemetry::{summarize_appservice_scheduler_metrics, AppserviceSchedulerTelemetrySummary};
use synapse_web::AppState;
use tokio::signal;

use synapse_storage::*;

mod database;
mod router;
mod services;
/// The `telemetry` module.
pub mod telemetry;

const MIN_DEHYDRATED_DEVICE_CLEANUP_INTERVAL_SECS: u64 = 300;

// --- Tuning constants ---

/// Fallback interval (seconds) for background maintenance task ticks.
///
/// Used only when the configured `server.background_tasks_interval` is zero
/// (e.g. unset or invalid). Default in [`ServerConfig`] is 60s.
const BACKGROUND_TASK_INTERVAL_SECS: u64 = 60;

/// Minimum interval (seconds) between background task executions to prevent
/// tight loops when a task completes quickly.
const MIN_BACKGROUND_INTERVAL_SECS: u64 = 10;

/// Capacity of the tokio broadcast channel used for graceful shutdown signaling.
const SHUTDOWN_BROADCAST_CAPACITY: usize = 3;

/// Fallback for [`ServerConfig::federation_retry_max_count`] when config is 0.
const FEDERATION_RETRY_MAX_COUNT: u64 = 5;

/// Fallback for [`ServerConfig::drain_timeout_secs`] when config is 0.
const DRAIN_TIMEOUT_SECS: u64 = 30;

/// Fallback for [`ServerConfig::megolm_cleanup_interval_secs`] when config is 0.
const MEGOLM_CLEANUP_INTERVAL_SECS: u64 = 6 * 3600;

/// Fallback for [`ServerConfig::pruning_interval_secs`] when config is 0.
const PRUNING_INTERVAL_SECS: u64 = 86400;

/// T03 MSC4140: default polling interval (seconds) for the delayed-event
/// dispatcher when `server.delayed_event_dispatch_interval_secs` is unset/zero.
const DEFAULT_DELAYED_EVENT_DISPATCH_INTERVAL_SECS: u64 = 5;

/// T03 MSC4140: maximum number of due delayed events processed per dispatch cycle.
const DELAYED_EVENT_DISPATCH_BATCH_SIZE: i64 = 100;

/// Helper macro for pruning background tasks.
/// Each pruning operation follows the same pattern: call an async function,
/// log success with a count, or log a warning on failure.
macro_rules! prune_step {
    ($label:expr, $prune_fn:expr) => {{
        match $prune_fn.await {
            Ok(count) => {
                if count > 0 {
                    tracing::info!(
                        count = count,
                        "{}: pruned {count} expired entries",
                        $label
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "{}: prune operation failed",
                    $label
                );
            }
        }
    }};
}

fn global_maintenance_tasks_enabled() -> bool {
    !matches!(
        std::env::var("SYNAPSE_ENABLE_GLOBAL_MAINTENANCE_TASKS").ok().as_deref(),
        Some("0" | "false" | "FALSE" | "False" | "off" | "OFF" | "Off")
    )
}

#[derive(Clone)]
struct PrometheusMetricsState {
    metrics: Arc<crate::common::metrics::MetricsCollector>,
    app_service_manager: Arc<synapse_services::application_service::ApplicationServiceManager>,
}

fn dehydrated_device_cleanup_interval(configured_interval_secs: u64) -> Duration {
    Duration::from_secs(configured_interval_secs.max(MIN_DEHYDRATED_DEVICE_CLEANUP_INTERVAL_SECS))
}

/// The `SynapseServer` struct.
pub struct SynapseServer {
    app_state: Arc<AppState>,
    router: Router,
    address: SocketAddr,
    federation_address: SocketAddr,
    media_path: std::path::PathBuf,
    scheduled_tasks: Arc<ScheduledTasks>,
    _rate_limit_config_manager: Option<Arc<RateLimitConfigManager>>,
    _config_watcher_handle: Option<tokio::task::JoinHandle<()>>,
}

impl SynapseServer {
    /// Create a new [`SynapseServer`] instance.
    /// Create a new [`SynapseServer`] instance.
    pub async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        // Make CORS origins from homeserver.yaml visible to the security check
        // BEFORE we run validation, so operators don't have to also set
        // ALLOWED_ORIGINS env var when they have already configured the file.
        set_config_allowed_origins(config.cors.allowed_origins.clone());
        set_bind_address(config.server.host.clone());

        let trust_forwarded = std::env::var("TRUST_FORWARDED_HEADERS")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        set_trust_forwarded_headers(trust_forwarded);

        let cors_report = check_cors_security();
        log_cors_security_report(&cors_report);

        if !cors_report.errors.is_empty() {
            let e = cors_report.errors.join("; ");
            ::tracing::error!("CORS configuration validation failed: {}", e);
            return Err(e.into());
        }

        if let Err(e) = validate_bind_address_for_dev_mode(&config.server.host) {
            ::tracing::warn!("{}", e);
        }

        let pool = match database::build_database_pool(&config).await {
            Ok(pool) => pool,
            Err(e) => {
                tracing::error!("Failed to initialize database: {e}");
                std::process::exit(1);
            }
        };
        let pool = Arc::new(pool);

        // Validate TOKEN_HASH_SECRET before accepting any requests.
        // In production, a missing or weak secret is a fatal startup error.
        if let Err(e) = synapse_common::crypto::validate_token_hash_secret() {
            return Err(format!("FATAL: {e}").into());
        }
        ::tracing::info!("[启动阶段 2/4] 安全密钥校验通过 (TOKEN_HASH_SECRET)");

        // Validate the worker HTTP replication secret before that surface can be
        // reached. `replication_http_auth_middleware` accepts a single shared secret
        // for replication positions, the event stream and worker/task state, so a
        // value published in this repository (e.g. the `worker_replication_secret_2026`
        // test fixture) is an authentication bypass, not a cosmetic issue. Fatal in
        // release builds only: the check is skipped entirely unless the worker surface
        // is mounted (`worker.enabled && worker.replication.http.enabled`), and dev
        // configs legitimately use short fixtures.
        // Topology validation below only logs this condition as a warning, so it
        // cannot gate startup on its own.
        if let Err(e) = worker_replication_secret_check(&config.worker) {
            return Err(format!("FATAL: {e}").into());
        }
        ::tracing::info!("[启动阶段 2/4] 安全密钥校验通过 (worker.replication.http)");

        ::tracing::info!("[启动阶段 2/4] 构建服务容器 (services + cache + redis)...");
        let (services, cache, redis_pool_option) = services::build_service_container(&pool, &config).await?;
        ::tracing::info!("[启动阶段 2/4] 服务容器构建完成");

        // Startup topology validation — ensures worker configuration is consistent before proceeding
        {
            let validation = synapse_services::worker::topology_validator::validate_worker_config(&config.worker);
            validation.log();
            if !validation.valid {
                ::tracing::warn!(
                    "Topology validation failed — check worker configuration. \
                     The server will continue to start, but the worker topology may be misconfigured."
                );
            }
        }
        if !config.server.app_service_config_files.is_empty() {
            let imported_services = services
                .admin
                .modules
                .app_service_manager
                .load_from_config_files(&config.server.app_service_config_files)
                .await?;
            ::tracing::info!(
                imported = imported_services.len(),
                "Imported application service configs from app_service_config_files"
            );
        }
        let app_state = Arc::new(AppState::new(services, cache));

        // Create the graceful-shutdown broadcast channel early so it can be
        // wired into AppState. This allows `POST /_synapse/admin/v1/restart`
        // to trigger a clean shutdown that the process manager (Docker /
        // systemd) can restart from.
        let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(SHUTDOWN_BROADCAST_CAPACITY);
        let app_state = Arc::new((*app_state).clone().with_shutdown_signal(shutdown_tx.clone()));

        let rate_limit_config_path = std::path::PathBuf::from(
            std::env::var("RATE_LIMIT_CONFIG_PATH").unwrap_or_else(|_| "/app/config/rate_limit.yaml".to_string()),
        );

        // Fallback used when the dedicated file is missing or unparseable:
        // honour the `rate_limit:` section of `homeserver.yaml` instead of
        // hard-coded `Default` values.
        //
        // Previously this path built `RateLimitConfigFile::default()`, so an
        // operator who declared limits in homeserver.yaml (the documented place)
        // had them silently replaced by built-in constants — neither file was
        // read. `trusted_proxies` / `trust_forwarded` now flow through too.
        //
        // `RateLimitConfigFile` remains the single in-memory representation the
        // middleware consults; this merely seeds it from the runtime view.
        let fallback_from_homeserver = |path: &std::path::Path, why: &str| -> (Arc<RateLimitConfigManager>, u64) {
            let converted = RateLimitConfigFile::from(&config.rate_limit);
            let reload_secs = converted.reload_interval_seconds;
            let manager = Arc::new(RateLimitConfigManager::new(converted, path.to_path_buf()));
            ::tracing::warn!(
                target: "security_audit",
                event = "rate_limit_config_fallback",
                path = %path.display(),
                reason = why,
                "限流配置以 homeserver.yaml 的 rate_limit 段为准（{}）；                 rate_limit_config_source_is_file=0 表示专题文件未生效",
                why
            );
            (manager, reload_secs)
        };

        let (rate_limit_config_manager, config_watcher_handle) = if rate_limit_config_path.exists() {
            match RateLimitConfigManager::from_file(&rate_limit_config_path).await {
                Ok(manager) => {
                    let manager = Arc::new(manager);
                    let config = manager.get_config();
                    let handle = start_config_watcher(manager.clone(), config.reload_interval_seconds).await;
                    ::tracing::info!("[启动阶段 3/4] 限流配置加载完成: {:?}", rate_limit_config_path);
                    (Some(manager), Some(handle))
                }
                Err(e) => {
                    // Degraded: the operator's file exists but could not be
                    // parsed. Log at error level (not warn) and surface it via
                    // `rate_limit_config_degraded` + /health so it cannot go
                    // unnoticed in production.
                    ::tracing::error!(
                        target: "security_audit",
                        event = "rate_limit_config_degraded",
                        path = %rate_limit_config_path.display(),
                        error = %e,
                        "[启动阶段 3/4] 限流配置解析失败，回退到 homeserver.yaml 的 rate_limit 段"
                    );
                    let (manager, reload_secs) = fallback_from_homeserver(&rate_limit_config_path, "parse error");
                    // Keep the watcher running: the file is present but broken,
                    // so a fix should be picked up without a restart.
                    let handle = start_config_watcher(manager.clone(), reload_secs).await;
                    (Some(manager), Some(handle))
                }
            }
        } else {
            ::tracing::warn!(
                target: "security_audit",
                event = "rate_limit_config_fallback",
                path = %rate_limit_config_path.display(),
                "[启动阶段 3/4] 限流专题文件不存在，以 homeserver.yaml 的 rate_limit 段为准"
            );
            let (manager, reload_secs) = fallback_from_homeserver(&rate_limit_config_path, "file missing");
            // Watch anyway: if the dedicated file appears later (volume mounted
            // after boot, config-management catching up) it takes over without a
            // restart. Previously no watcher was started here, so the fallback
            // was permanent for the process lifetime.
            let handle = start_config_watcher(manager.clone(), reload_secs).await;
            (Some(manager), Some(handle))
        };

        let app_state = if let Some(ref manager) = rate_limit_config_manager {
            Arc::new((*app_state).clone().with_rate_limit_config(manager.clone()))
        } else {
            app_state
        };

        // Surface the effective source as a gauge so dashboards/alerts can
        // distinguish "config file in effect" from "built-in defaults in use".
        // `1` = file, `0` = degraded to defaults.
        {
            let manager = rate_limit_config_manager.as_ref();
            let is_file =
                manager.is_some_and(|m| m.degradation().source == crate::common::rate_limit_config::ConfigSource::File);
            app_state
                .services
                .core
                .metrics
                .register_gauge("rate_limit_config_source_is_file".to_string())
                .set(if is_file { 1.0 } else { 0.0 });
            if !is_file {
                ::tracing::error!(
                    target: "security_audit",
                    event = "rate_limit_config_degraded",
                    "_matrix_hint" = "rate_limit_config_source_is_file=0",
                    "限流配置未从文件生效：rate_limit_config_source_is_file=0，\
                     运维写入的规则已被内置默认值取代"
                );
            }
        }

        let scheduled_tasks = Arc::new(ScheduledTasks::from_config(
            Arc::new(Database::from_pool((*pool).clone(), redis_pool_option)),
            &config.server,
            Some(app_state.services.core.server_metrics.clone()),
        ));

        let address = format!("{}:{}", config.server.host, config.server.port).parse::<SocketAddr>()?;
        let federation_address =
            format!("{}:{}", config.server.host, config.federation.federation_port).parse::<SocketAddr>()?;
        let media_path = std::path::PathBuf::from(&config.server.media_path);

        let router = router::build_router((*app_state).clone(), &config);

        Ok(Self {
            app_state,
            router,
            address,
            federation_address,
            media_path,
            scheduled_tasks,
            _rate_limit_config_manager: rate_limit_config_manager,
            _config_watcher_handle: config_watcher_handle,
        })
    }

    /// Run the server event loop.
    /// Run the server event loop.
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.log_startup_banner();

        if let Err(e) = self.warmup().await {
            ::tracing::warn!("Warmup encountered minor errors: {}", e);
        }

        let (current_worker_type, maintenance_owner, maintenance_runtime_enabled, run_global_maintenance) =
            self.eval_maintenance_state();
        ::tracing::info!(
            worker_type = current_worker_type.as_str(),
            maintenance_owner = maintenance_owner.as_str(),
            maintenance_runtime_enabled,
            run_global_maintenance,
            "Evaluated global maintenance task ownership"
        );

        if run_global_maintenance {
            self.app_state
                .services
                .federation
                .key_rotation_manager
                .start_auto_rotation(self.app_state.services.shutdown_token.clone())
                .await;
            ::tracing::info!("Starting scheduled database monitoring and maintenance tasks...");
            self.scheduled_tasks.start_all(self.app_state.services.shutdown_token.clone());
            // P0-2: Spawn pool metrics periodic task (30s interval) to wire
            // update_pool_metrics → ServerMetrics. This restores visibility
            // into db_connections_active/idle, pool_utilization, and
            // pool_health_status for Prometheus/Grafana alerting.
            let shutdown = self.app_state.services.shutdown_token.clone();
            let server_metrics = self.app_state.services.core.server_metrics.clone();
            let database = self.scheduled_tasks.database.clone();
            tokio::spawn(async move {
                use std::time::Duration;
                let mut timer = tokio::time::interval(Duration::from_secs(30));
                timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tokio::select! {
                        biased;
                        _ = shutdown.cancelled() => {
                            tracing::info!("pool metrics task exiting on shutdown");
                            break;
                        }
                        _ = timer.tick() => {
                            let pool_ref = database.pool();
                            let pool_size = pool_ref.size();
                            let idle = pool_ref.num_idle() as u32;
                            let active = pool_size.saturating_sub(idle);
                            let max_size = pool_ref.options().get_max_connections();
                            let utilization = if max_size > 0 {
                                (pool_size as f64) / (max_size as f64)
                            } else {
                                0.0
                            };
                            // Health status: assume healthy if we can read pool stats.
                            // Real health is checked by the health_check task.
                            let is_healthy = true;
                            server_metrics.update_pool_metrics(
                                active as f64,
                                idle as f64,
                                utilization,
                                is_healthy,
                            );
                            tracing::debug!(
                                "pool metrics updated: size={}, idle={}, active={}, max={}, util={:.3}",
                                pool_size, idle, active, max_size, utilization
                            );
                        }
                    }
                }
            });
        } else {
            ::tracing::info!(
                worker_type = current_worker_type.as_str(),
                maintenance_owner = maintenance_owner.as_str(),
                maintenance_runtime_enabled,
                "Skipping global maintenance tasks on this worker instance"
            );
        }

        #[cfg(feature = "beacons")]
        let beacon_service = self.app_state.services.rooms.beacon_service.clone();
        let background_tasks_interval =
            self.app_state.services.core.config.server.background_tasks_interval.max(MIN_BACKGROUND_INTERVAL_SECS);
        if run_global_maintenance {
            let retention_service = self.app_state.services.admin.modules.retention_service.clone();
            let retention_config = self.app_state.services.core.config.retention.clone();
            let lifecycle_interval_secs = if retention_config.lifecycle_cleanup_enabled {
                retention_config.lifecycle_cleanup_interval_secs.max(background_tasks_interval)
            } else {
                background_tasks_interval
            };
            let megolm_service = self.app_state.services.e2ee.megolm_service.clone();
            let mut shutdown_rx0 = self
                .app_state
                .shutdown_signal
                .as_ref()
                .ok_or("shutdown signal must be wired into AppState at construction time")?
                .subscribe();

            tokio::spawn(async move {
                let mut interval_timer = tokio::time::interval(Duration::from_secs(lifecycle_interval_secs));
                interval_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tokio::select! {
                        _ = interval_timer.tick() => {
                            if retention_config.lifecycle_cleanup_enabled {
                                #[cfg(feature = "beacons")]
                                {
                                    retention_service.run_data_lifecycle_cycle(&beacon_service, &retention_config).await;
                                }
                                #[cfg(not(feature = "beacons"))]
                                {
                                    retention_service.run_data_lifecycle_cycle_no_beacons(&retention_config).await;
                                }
                            } else {
                                #[cfg(feature = "beacons")]
                                match beacon_service.cleanup_expired_beacons().await {
                                    Ok(count) => {
                                        if count > 0 {
                                            ::tracing::info!("Cleaned up {} expired beacons", count);
                                        }
                                    }
                                    Err(error) => {
                                        ::tracing::warn!("Failed to cleanup expired beacons: {}", error);
                                    }
                                }
                            }

                            // Clean up expired Megolm sessions to prevent unbounded table growth
                            match megolm_service.cleanup_expired_sessions().await {
                                Ok(count) => {
                                    if count > 0 {
                                        ::tracing::info!("Cleaned up {} expired Megolm sessions", count);
                                    }
                                }
                                Err(error) => {
                                    ::tracing::warn!("Failed to cleanup expired Megolm sessions: {}", error);
                                }
                            }
                        }
                        _ = shutdown_rx0.recv() => {
                            ::tracing::info!("Retention/Megolm cleanup task shutting down");
                            break;
                        }
                    }
                }
            });
        }

        let router = self.router.clone();
        let fed_router = self.router.clone();
        // Reuse the shutdown broadcast sender wired into AppState at
        // construction time. `POST /_synapse/admin/v1/restart` sends on this
        // channel to trigger a graceful shutdown.
        let shutdown_tx = self
            .app_state
            .shutdown_signal
            .clone()
            .ok_or("shutdown signal must be wired into AppState at construction time")?;

        let client_listener = tokio::net::TcpListener::bind(self.address).await?;
        let federation_listener = tokio::net::TcpListener::bind(self.federation_address).await?;
        let prometheus_listener = self.bind_prometheus_listener_if_enabled().await?;

        let (client_tx, client_rx) = tokio::sync::oneshot::channel();
        let (fed_tx, fed_rx) = tokio::sync::oneshot::channel();
        let (prom_tx, prom_rx) = tokio::sync::oneshot::channel();

        let mut shutdown_rx1 = shutdown_tx.subscribe();
        let mut shutdown_rx2 = shutdown_tx.subscribe();
        let mut shutdown_rx3 = shutdown_tx.subscribe();
        let mut shutdown_rx4 = shutdown_tx.subscribe();
        let mut shutdown_rx5 = shutdown_tx.subscribe();
        let mut shutdown_rx6 = shutdown_tx.subscribe();
        let mut shutdown_rx7 = shutdown_tx.subscribe();
        // T03 MSC4140: dedicated shutdown receiver for delayed-event dispatcher
        // (separate from shutdown_rx6 used by Megolm session cleanup to avoid move conflict)
        let mut shutdown_rx_delayed = shutdown_tx.subscribe();
        let mut shutdown_rx_drain_gate = shutdown_tx.subscribe();

        if run_global_maintenance {
            let bg_service = self.app_state.services.admin.modules.background_update_service.clone();
            let retention_service = self.app_state.services.admin.modules.retention_service.clone();
            let media_service = self.app_state.services.core.media_service.clone();
            let event_broadcaster = self.app_state.services.core.event_broadcaster.clone();
            let remote_media_lifetime = self.app_state.services.core.config.server.remote_media_lifetime;
            let local_media_lifetime = self.app_state.services.core.config.server.local_media_lifetime;
            let configured_bg_interval = self.app_state.services.core.config.server.background_tasks_interval;
            let bg_tick_secs =
                if configured_bg_interval > 0 { configured_bg_interval } else { BACKGROUND_TASK_INTERVAL_SECS };
            let configured_fed_retry = self.app_state.services.core.config.server.federation_retry_max_count;
            let fed_retry_threshold =
                if configured_fed_retry > 0 { configured_fed_retry } else { FEDERATION_RETRY_MAX_COUNT };
            let mut media_cleanup_counter: u64 = 0;
            let mut federation_retry_counter: u64 = 0;
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(bg_tick_secs));
                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            if let Err(e) = bg_service.retry_failed().await {
                                ::tracing::warn!("Background update retry failed: {}", e);
                            }
                            if let Err(e) = bg_service.cleanup_expired_locks().await {
                                ::tracing::warn!("Background lock cleanup failed: {}", e);
                            }
                            if let Err(e) = retention_service.run_scheduled_cleanups().await {
                                ::tracing::warn!("Retention cleanup failed: {}", e);
                            }
                            media_cleanup_counter += 1;
                            if media_cleanup_counter >= 60 {
                                media_cleanup_counter = 0;
                                if remote_media_lifetime > 0 {
                                    let cutoff_ts = current_timestamp_millis()
                                        - (remote_media_lifetime as i64 * 1000);
                                    if let Err(e) = media_service.purge_media_cache(cutoff_ts).await {
                                        ::tracing::warn!("Remote media cleanup failed: {}", e);
                                    }
                                }
                                if local_media_lifetime > 0 {
                                    let cutoff_ts = current_timestamp_millis()
                                        - (local_media_lifetime as i64 * 1000);
                                    if let Err(e) = media_service.purge_media_cache(cutoff_ts).await {
                                        ::tracing::warn!("Local media cleanup failed: {}", e);
                                    }
                                }
                            }
                            federation_retry_counter += 1;
                            if federation_retry_counter >= fed_retry_threshold {
                                federation_retry_counter = 0;
                                if let Ok(retried) = event_broadcaster.retry_pending_transactions().await {
                                    if retried > 0 {
                                        ::tracing::info!("Federation retry: {} transactions retried", retried);
                                    }
                                }
                            }
                        }
                        _ = shutdown_rx4.recv() => {
                            ::tracing::info!("Background task scheduler shutting down");
                            break;
                        }
                    }
                }
            });
        }

        if run_global_maintenance {
            let dehydrated_service = self.app_state.services.e2ee.dehydrated_device_service.clone();
            let dehydrated_cleanup_interval_secs =
                self.app_state.services.core.config.server.dehydrated_device_cleanup_interval_secs;
            let cleanup_interval = dehydrated_device_cleanup_interval(dehydrated_cleanup_interval_secs);
            let server_metrics = self.app_state.services.core.server_metrics.clone();
            tokio::spawn(async move {
                let mut interval_timer = tokio::time::interval(cleanup_interval);
                interval_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                interval_timer.tick().await; // skip immediate tick after startup

                loop {
                    tokio::select! {
                        _ = interval_timer.tick() => {
                            server_metrics.dehydrated_device_cleanup_total.inc();
                            let start_time = Instant::now();
                            match dehydrated_service.sweep_expired().await {
                                Ok(0) => ::tracing::debug!(
                                    message = "Dehydrated device cleanup task: no expired devices found for sweep"
                                ),
                                Ok(n) => {
                                    ::tracing::info!(
                                        message = "Swept expired dehydrated device(s)",
                                        devices_swept = n
                                    );
                                    server_metrics.dehydrated_device_cleaned_total.inc_by(n);
                                }
                                Err(e) => {
                                    ::tracing::warn!(
                                        message = "Dehydrated device expiry sweep failed",
                                        error = %e
                                    );
                                    server_metrics.dehydrated_device_cleanup_errors_total.inc();
                                }
                            }
                            server_metrics.dehydrated_device_cleanup_duration.observe(start_time.elapsed().as_millis() as f64);
                        }
                        _ = shutdown_rx5.recv() => {
                            ::tracing::info!("Dehydrated device cleanup task shutting down");
                            break;
                        }
                    }
                }
            });
        }

        if run_global_maintenance {
            // Megolm session expiry cleanup: periodically delete expired megolm
            // sessions to prevent unbounded growth of the megolm_sessions table.
            // Expired sessions should be cleaned up
            // automatically. Runs every 6 hours by default.
            let key_rotation_storage = self.app_state.services.core.key_rotation_storage.clone();
            let configured_megolm_secs = self.app_state.services.core.config.server.megolm_cleanup_interval_secs;
            let megolm_interval_secs =
                if configured_megolm_secs > 0 { configured_megolm_secs } else { MEGOLM_CLEANUP_INTERVAL_SECS };
            tokio::spawn(async move {
                let mut interval_timer = tokio::time::interval(tokio::time::Duration::from_secs(megolm_interval_secs));
                interval_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                interval_timer.tick().await; // skip immediate tick after startup

                loop {
                    tokio::select! {
                        _ = interval_timer.tick() => {
                            match key_rotation_storage.delete_expired_sessions().await {
                                Ok(0) => {
                                    ::tracing::debug!("Megolm session cleanup: no expired sessions found");
                                }
                                Ok(n) => {
                                    ::tracing::info!(
                                        deleted_sessions = n,
                                        "Megolm session cleanup: deleted expired sessions"
                                    );
                                }
                                Err(e) => {
                                    ::tracing::warn!(
                                        error = %e,
                                        "Megolm session cleanup failed"
                                    );
                                }
                            }
                        }
                        _ = shutdown_rx6.recv() => {
                            ::tracing::info!("Megolm session cleanup task shutting down");
                            break;
                        }
                    }
                }
            });
        }

        if run_global_maintenance {
            // Background pruning of append-only / stale tables to prevent
            // disk bloat on long-running instances. Prunes:
            //   - device_lists_changes older than 30 days
            //   - presence records inactive beyond the presence prune timeout
            //   - one-time keys that are used or older than 7 days
            // Runs daily.
            let pruning_pool = self.app_state.services.account.user_storage.pool().clone();
            let configured_pruning_secs = self.app_state.services.core.config.server.pruning_interval_secs;
            let pruning_interval_secs =
                if configured_pruning_secs > 0 { configured_pruning_secs } else { PRUNING_INTERVAL_SECS };
            tokio::spawn(async move {
                let mut interval_timer = tokio::time::interval(Duration::from_secs(pruning_interval_secs));
                interval_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                interval_timer.tick().await; // skip immediate tick after startup

                loop {
                    tokio::select! {
                        _ = interval_timer.tick() => {
                            prune_step!("device list changes", synapse_storage::pruning::prune_old_device_list_changes(&pruning_pool, synapse_storage::pruning::DEVICE_LIST_CHANGES_RETENTION_DAYS));

                            prune_step!("device list stream", synapse_storage::pruning::prune_old_device_lists_stream(&pruning_pool));

                            prune_step!("device list outbound pokes", synapse_storage::pruning::prune_sent_device_lists_outbound_pokes(&pruning_pool));

                            prune_step!("presence", synapse_storage::pruning::prune_expired_presence(&pruning_pool));

                            prune_step!("one-time keys", synapse_storage::pruning::prune_expired_one_time_keys(&pruning_pool));

                            // Extended pruning for additional append-only
                            // tables that accumulate without bound on long-running
                            // instances.
                            prune_step!("to-device transactions", synapse_storage::pruning::prune_old_to_device_transactions(&pruning_pool));

                            prune_step!("token blacklist", synapse_storage::pruning::prune_expired_token_blacklist(&pruning_pool));

                            prune_step!("federation queue", synapse_storage::pruning::prune_old_federation_queue(&pruning_pool));

                            prune_step!("quarantined media changes", synapse_storage::pruning::prune_old_quarantined_media_changes(&pruning_pool));
                        }
                        _ = shutdown_rx7.recv() => {
                            ::tracing::info!("Database pruning task shutting down");
                            break;
                        }
                    }
                }
            });
        }

        tokio::spawn(async move {
            let _ = shutdown_tx;
            axum::serve(client_listener, router.into_make_service_with_connect_info::<SocketAddr>())
                .with_graceful_shutdown(async move {
                    shutdown_rx1.recv().await.ok();
                })
                .await
                .ok();
            let _ = client_tx.send(());
        });

        // T03 MSC4140: Delayed event dispatcher — polls scheduled events and injects them
        // into the room's message pipeline. Uses Redis distributed lock to prevent duplicate dispatch.
        let delayed_event_storage = self.app_state.services.admin.modules.delayed_event_storage.clone();
        let room_service = self.app_state.services.rooms.room_service.clone();
        let cache = self.app_state.services.core.cache.clone();
        let delayed_event_dispatch_interval =
            self.app_state.services.core.config.server.delayed_event_dispatch_interval_secs;
        let dispatch_interval_secs = if delayed_event_dispatch_interval > 0 {
            delayed_event_dispatch_interval
        } else {
            DEFAULT_DELAYED_EVENT_DISPATCH_INTERVAL_SECS
        };
        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(tokio::time::Duration::from_secs(dispatch_interval_secs));
            interval_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            interval_timer.tick().await; // skip immediate tick after startup

            loop {
                tokio::select! {
                    _ = interval_timer.tick() => {
                        let cycle_start = Instant::now();
                        let now_ms = current_timestamp_millis();

                        // Fetch due events from storage
                        let due_events = match delayed_event_storage.get_due_events(now_ms, DELAYED_EVENT_DISPATCH_BATCH_SIZE).await {
                            Ok(events) => events,
                            Err(e) => {
                                ::tracing::error!("Failed to get due delayed events: {}", e);
                                continue;
                            }
                        };

                        if due_events.is_empty() {
                            continue;
                        }

                        let mut dispatched: u64 = 0;
                        let mut errors: u64 = 0;
                        let mut skipped_contention: u64 = 0;

                        for event in due_events {
                            if event.status != "pending" {
                                continue;
                            }

                            // Use distributed lock to prevent duplicate dispatch across instances
                            let lock_key = format!("delayed:event:dispatch:{}", event.id);
                            let lock_acquired = cache.try_acquire_lock(&lock_key, 30).await.unwrap_or(false);

                            if !lock_acquired {
                                skipped_contention += 1;
                                continue;
                            }

                            // Dispatch the event: use the room service's messaging to create the event
                            // The synthetic event_id from delayed_events is used as the txn_id for deduplication
                            let send_result = room_service
                                .messaging()
                                .send_message_with_txn(
                                    &event.room_id,
                                    &event.user_id,
                                    &event.event_type,
                                    &event.content,
                                    &event.event_id, // txn_id for dedup (synthetic placeholder)
                                )
                                .await;

                            match send_result {
                                Ok(_) => {
                                    if let Ok(true) = delayed_event_storage.mark_sent(event.id).await {
                                        dispatched += 1;
                                        ::tracing::debug!("MSC4140 dispatched delayed event {}", event.id);
                                    }
                                }
                                Err(e) => {
                                    ::tracing::error!("Failed to dispatch delayed event {}: {}", event.id, e);
                                    errors += 1;
                                }
                            }

                            // Release lock (TTL handles it, but explicit release is cleaner)
                            let _ = cache.release_lock(&lock_key).await;
                        }

                        ::tracing::info!(
                            "[MSC4140] delayed_event_dispatch: dispatched={}, errors={}, contention_skipped={}, elapsed_ms={:?}",
                            dispatched,
                            errors,
                            skipped_contention,
                            cycle_start.elapsed()
                        );
                    }
                    _ = shutdown_rx_delayed.recv() => {
                        ::tracing::info!("Delayed event dispatcher shutting down");
                        break;
                    }
                }
            }
        });

        tokio::spawn(async move {
            axum::serve(federation_listener, fed_router.into_make_service_with_connect_info::<SocketAddr>())
                .with_graceful_shutdown(async move {
                    shutdown_rx2.recv().await.ok();
                })
                .await
                .ok();
            let _ = fed_tx.send(());
        });

        if let Some(prometheus_listener) = prometheus_listener {
            let prometheus_path = self.app_state.services.core.config.prometheus.path.clone();
            let metrics_state = PrometheusMetricsState {
                metrics: self.app_state.services.core.metrics.clone(),
                app_service_manager: self.app_state.services.admin.modules.app_service_manager.clone(),
            };
            // 读取 PROMETHEUS_AUTH_TOKEN 环境变量，为空时不启用鉴权（向后兼容）
            let prometheus_auth_token = std::env::var("PROMETHEUS_AUTH_TOKEN").ok().filter(|s| !s.is_empty());
            let prometheus_router = Router::new()
                .route(&prometheus_path, get(render_prometheus_metrics))
                .with_state(metrics_state)
                .layer(middleware::from_fn(prometheus_auth_middleware))
                .layer(Extension(prometheus_auth_token));

            tokio::spawn(async move {
                axum::serve(prometheus_listener, prometheus_router.into_make_service())
                    .with_graceful_shutdown(async move {
                        shutdown_rx3.recv().await.ok();
                    })
                    .await
                    .ok();
                let _ = prom_tx.send(());
            });
        } else {
            let _ = prom_tx.send(());
        }

        ::tracing::info!("[启动完成] ✅ Synapse Rust Matrix Server 已启动并准备接受请求 (4/4 阶段全部完成)");

        self.spawn_shutdown_signal_listener(shutdown_tx.clone());
        shutdown_rx_drain_gate.recv().await.ok();

        // Wait for all listeners to drain, with a hard cap to prevent
        // long-polling endpoints (e.g. /sync with 90s+ timeout) from blocking
        // rolling updates indefinitely.
        let configured_drain_secs = self.app_state.services.core.config.server.drain_timeout_secs;
        Self::await_listeners_drained(client_rx, fed_rx, prom_rx, configured_drain_secs).await;
        ::tracing::info!("Servers shutdown complete");

        self.log_worker_exit_summary();
        Ok(())
    }

    async fn warmup(&self) -> Result<(), Box<dyn std::error::Error>> {
        let pool = self.app_state.services.account.user_storage.pool();

        ::tracing::info!("Performing system warmup...");

        sqlx::query_scalar::<_, i32>("SELECT 1 AS health_check").fetch_one(&**pool).await?;

        let _ = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM users").fetch_one(&**pool).await?;

        #[cfg(feature = "saml-sso")]
        {
            if let Err(e) = self.app_state.services.sso.saml_service.hydrate_runtime_overrides().await {
                ::tracing::warn!(
                    "Failed to hydrate SAML runtime config overrides: {}. Continuing with base config.",
                    e
                );
            }
        }

        ::tracing::info!("Warmup completed successfully.");
        Ok(())
    }

    /// Emit the structured "[启动阶段 4/4]" startup banner describing the
    /// active listeners and the media storage path. Extracted from `run()` so
    /// that the boot sequence body stays focused on sequencing.
    fn log_startup_banner(&self) {
        ::tracing::info!("[启动阶段 4/4] 正在启动 Synapse Rust Matrix Server...");
        ::tracing::info!("[启动阶段 4/4] Server name: {}", self.app_state.services.core.server_name);
        ::tracing::info!("[启动阶段 4/4] Listening on (Client API): {}", self.address);
        ::tracing::info!("[启动阶段 4/4] Listening on (Federation): {}", self.federation_address);
        if self.app_state.services.core.config.prometheus.enabled {
            ::tracing::info!(
                "[启动阶段 4/4] Listening on (Prometheus): {}:{}{}",
                self.app_state.services.core.config.server.host,
                self.app_state.services.core.config.prometheus.port,
                self.app_state.services.core.config.prometheus.path
            );
        }
        ::tracing::info!("[启动阶段 4/4] Media storage: {}", self.media_path.display());
    }

    /// Wait for the client, federation, and prometheus listeners to report
    /// graceful drain completion, with a hard timeout cap. Extracted from
    /// `run()` so the drain logic is independently testable and reviewable.
    ///
    /// `configured_drain_secs == 0` falls back to the default `DRAIN_TIMEOUT_SECS`.
    /// Long-polling endpoints (e.g. /sync with 90s+ timeout) must not be allowed
    /// to block rolling updates indefinitely; the cap is the safety net.
    async fn await_listeners_drained(
        client_rx: tokio::sync::oneshot::Receiver<()>,
        fed_rx: tokio::sync::oneshot::Receiver<()>,
        prom_rx: tokio::sync::oneshot::Receiver<()>,
        configured_drain_secs: u64,
    ) {
        let drain_secs = if configured_drain_secs > 0 { configured_drain_secs } else { DRAIN_TIMEOUT_SECS };
        let drain_timeout = Duration::from_secs(drain_secs);
        let drain_result = tokio::time::timeout(drain_timeout, async {
            client_rx.await.ok();
            fed_rx.await.ok();
            prom_rx.await.ok();
        })
        .await;
        if drain_result.is_err() {
            ::tracing::warn!(
                target: "shutdown",
                "Graceful drain timed out after {drain_timeout:?} — forcing exit with in-flight requests"
            );
        }
    }

    /// Evaluate whether the current worker instance owns global maintenance tasks.
    ///
    /// Returns `(worker_type, maintenance_owner, maintenance_runtime_enabled, run_global_maintenance)`.
    /// Extracted from `run()` so the policy logic is independently reviewable
    /// and unit-testable without booting the whole server.
    fn eval_maintenance_state(&self) -> (String, String, bool, bool) {
        let worker_config = &self.app_state.services.core.config.worker;
        let current_worker_type = current_instance_worker_type(worker_config).as_str().to_string();
        let maintenance_owner = global_maintenance_owner(worker_config).as_str().to_string();
        let maintenance_runtime_enabled = global_maintenance_tasks_enabled();
        let run_global_maintenance = should_run_global_maintenance(worker_config) && maintenance_runtime_enabled;
        (current_worker_type, maintenance_owner, maintenance_runtime_enabled, run_global_maintenance)
    }

    /// Bind the Prometheus metrics listener if `prometheus.enabled` is true in config.
    ///
    /// Extracted from `run()` so the bind path is testable in isolation and
    /// the conditional listener setup is obvious at the call site.
    async fn bind_prometheus_listener_if_enabled(
        &self,
    ) -> Result<Option<tokio::net::TcpListener>, Box<dyn std::error::Error>> {
        let prometheus_config = &self.app_state.services.core.config.prometheus;
        if !prometheus_config.enabled {
            return Ok(None);
        }
        let listener = tokio::net::TcpListener::bind(format!(
            "{}:{}",
            self.app_state.services.core.config.server.host, prometheus_config.port
        ))
        .await?;
        Ok(Some(listener))
    }

    /// Emit a final structured log line capturing worker identity and restart count.
    ///
    /// Distinguishes crash-loop-prone worker types (`federation_reader`,
    /// `federation_sender`, `pusher`) at WARN level from routine shutdowns at
    /// INFO level. Extracted from `run()` so the audit-friendly summary is
    /// independently reviewable.
    fn log_worker_exit_summary(&self) {
        let worker_instance = std::env::var("WORKER_INSTANCE_NAME").unwrap_or_else(|_| "master".to_string());
        let restart_count = std::env::var("RESTART_COUNT").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        match worker_instance.as_str() {
            "federation_reader" | "federation_sender" | "pusher" => {
                ::tracing::warn!(
                    target: "worker_exit",
                    worker_instance = %worker_instance,
                    restart_count = %restart_count,
                    "Worker exiting — check container restart policy and upstream logs for crash-loop evidence"
                );
            }
            _ => {
                ::tracing::info!(
                    target: "worker_exit",
                    worker_instance = %worker_instance,
                    restart_count = %restart_count,
                    "Worker shutdown complete"
                );
            }
        }
    }

    /// Spawn the SIGINT/SIGTERM/Ctrl+C handler that triggers a graceful shutdown.
    ///
    /// Extracted from `run()` so the signal-path wiring is a one-liner at the
    /// call site and the underlying `spawn_shutdown_signal_handler` free
    /// function remains the single source of truth for the signal semantics.
    fn spawn_shutdown_signal_listener(&self, shutdown_tx: tokio::sync::broadcast::Sender<()>) {
        let shutdown_token = self.app_state.services.shutdown_token.clone();
        let worker_instance = std::env::var("WORKER_INSTANCE_NAME").unwrap_or_else(|_| "master".to_string());
        let start_ts = current_timestamp_millis();
        tokio::spawn(spawn_shutdown_signal_handler(shutdown_tx, shutdown_token, worker_instance, start_ts));
    }
}

/// Handle SIGINT / SIGTERM / Ctrl+C by notifying the shutdown channel and
/// cancelling the GracefulShutdownToken. Extracted from `run()` so the signal
/// logic is independently testable and audit-friendly.
async fn spawn_shutdown_signal_handler(
    shutdown_tx_signal: tokio::sync::broadcast::Sender<()>,
    shutdown_token: tokio_util::sync::CancellationToken,
    worker_instance: String,
    start_ts: i64,
) {
    let sig = tokio::select! {
        _ = signal::ctrl_c() => "SIGINT",
        sig = async {
            #[cfg(unix)]
            {
                use tokio::signal::unix::{signal, SignalKind};
                let mut sigterm = signal(SignalKind::terminate()).ok()?;
                sigterm.recv().await?;
                Some("SIGTERM")
            }
            #[cfg(not(unix))]
            None::<&str>
        } => sig.unwrap_or("SIGTERM"),
    };
    let uptime_secs = (current_timestamp_millis() - start_ts) / 1000;
    ::tracing::warn!(
        target: "shutdown",
        signal = %sig,
        worker_instance = %worker_instance,
        uptime_secs = %uptime_secs,
        "Shutdown signal received — draining listeners"
    );
    let _ = shutdown_tx_signal.send(());
    shutdown_token.cancel();
}

/// Prometheus metrics endpoint middleware — requires Bearer token if configured.
///
/// When `PROMETHEUS_AUTH_TOKEN` env var is set, all `/metrics` requests must include
/// `Authorization: Bearer <token>` header. Without the token, returns 401.
/// This prevents unauthenticated access to internal metrics (CPU, memory, request counts).
async fn prometheus_auth_middleware(
    Extension(auth_token): Extension<Option<String>>,
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Result<impl IntoResponse, StatusCode> {
    let Some(expected_token) = auth_token else {
        // No auth configured — allow unauthenticated access (backward compatible)
        return Ok(next.run(req).await);
    };

    let auth_header =
        req.headers().get(axum::http::header::AUTHORIZATION).and_then(|h| h.to_str().ok()).map(|s| s.to_string());

    match auth_header {
        Some(ref h) if h.starts_with("Bearer ") && &h[7..] == expected_token.as_str() => Ok(next.run(req).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

async fn render_prometheus_metrics(
    axum::extract::State(state): axum::extract::State<PrometheusMetricsState>,
) -> impl IntoResponse {
    let mut rendered = state.metrics.to_prometheus_format();

    match state.app_service_manager.get_statistics().await {
        Ok(appservice_statistics) => {
            let summary = summarize_appservice_scheduler_metrics(&appservice_statistics);
            rendered.push_str(&render_appservice_scheduler_prometheus_metrics(&summary));
        }
        Err(error) => {
            ::tracing::warn!(error = %error, "Failed to collect appservice scheduler metrics for Prometheus output");
        }
    }

    ([(http::header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")], rendered)
}

fn render_appservice_scheduler_prometheus_metrics(summary: &AppserviceSchedulerTelemetrySummary) -> String {
    let mut output = String::new();
    append_prometheus_gauge(
        &mut output,
        "synapse_appservice_scheduler_total_services",
        "Number of registered application services included in scheduler telemetry",
        summary.total_services as f64,
    );
    append_prometheus_gauge(
        &mut output,
        "synapse_appservice_scheduler_available_services",
        "Number of application services with scheduler state available",
        summary.scheduler_available_services as f64,
    );
    append_prometheus_gauge(
        &mut output,
        "synapse_appservice_scheduler_backoff_services",
        "Number of application services currently observed in retry backoff",
        summary.services_in_backoff as f64,
    );
    append_prometheus_gauge(
        &mut output,
        "synapse_appservice_scheduler_capacity_limited_services",
        "Number of application services most recently limited by scheduler capacity",
        summary.services_capacity_limited as f64,
    );
    append_prometheus_gauge(
        &mut output,
        "synapse_appservice_scheduler_services_with_pending_transactions",
        "Number of application services with pending transactions",
        summary.services_with_pending_transactions as f64,
    );
    append_prometheus_gauge(
        &mut output,
        "synapse_appservice_scheduler_pending_events",
        "Aggregated pending event count across application services",
        summary.total_pending_events as f64,
    );
    append_prometheus_gauge(
        &mut output,
        "synapse_appservice_scheduler_pending_transactions",
        "Aggregated pending transaction count across application services",
        summary.total_pending_transactions as f64,
    );
    append_prometheus_gauge(
        &mut output,
        "synapse_appservice_scheduler_success_count",
        "Aggregated scheduler success count across application services",
        summary.total_success_count as f64,
    );
    append_prometheus_gauge(
        &mut output,
        "synapse_appservice_scheduler_failure_count",
        "Aggregated scheduler failure count across application services",
        summary.total_failure_count as f64,
    );
    append_prometheus_gauge(
        &mut output,
        "synapse_appservice_scheduler_backoff_count",
        "Aggregated scheduler backoff count across application services",
        summary.total_backoff_count as f64,
    );
    append_prometheus_gauge(
        &mut output,
        "synapse_appservice_scheduler_capacity_limited_count",
        "Aggregated scheduler capacity-limited count across application services",
        summary.total_capacity_limited_count as f64,
    );
    append_prometheus_gauge(
        &mut output,
        "synapse_appservice_scheduler_in_flight_count",
        "Aggregated scheduler in-flight count across application services",
        summary.total_in_flight_count as f64,
    );
    output
}

fn append_prometheus_gauge(output: &mut String, name: &str, help: &str, value: f64) {
    output.push_str(&format!("# HELP {name} {help}\n"));
    output.push_str(&format!("# TYPE {name} gauge\n"));
    output.push_str(&format!("{name} {value}\n"));
}

/// Startup gate for the worker HTTP replication secret.
///
/// `validate_replication_http_secret` returns `Ok` unless the surface is mounted
/// (`worker.enabled && worker.replication.http.enabled`) and enforces the strength
/// policy (minimum length, known-published values) only in `strict` mode. Strength is
/// a release-only requirement because dev/test configs legitimately use short
/// fixtures.
///
/// Kept as a named helper rather than inlined in the bootstrap path so the
/// build-mode policy is unit-testable: writing `cfg!(debug_assertions)` here
/// (inverted) would silently disable the gate in production, and only a test that
/// runs in debug mode can catch that.
fn worker_replication_secret_check(worker: &synapse_common::config::worker::WorkerConfig) -> Result<(), String> {
    synapse_common::config::worker::validate_replication_http_secret(worker, !cfg!(debug_assertions))
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_services::worker::types::WorkerType;
    #[cfg(feature = "test-utils")]
    use synapse_storage::application_service::{ApplicationServiceStorage, RegisterApplicationServiceRequest};
    #[cfg(feature = "test-utils")]
    use synapse_test_utils::prepare_shared_test_pool;
    #[cfg(feature = "test-utils")]
    use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

    #[test]
    fn worker_replication_secret_check_skips_unmounted_surface() {
        // `docker/config/homeserver.yaml` ships `worker.enabled: false` with
        // `replication.http.enabled: true`; nothing is mounted there, so this gate
        // must not block startup even with no secret at all.
        let config = synapse_common::config::worker::WorkerConfig::default();
        assert!(!config.enabled);
        assert!(worker_replication_secret_check(&config).is_ok());
    }

    #[test]
    fn worker_replication_secret_check_requires_a_secret_when_mounted_even_in_debug() {
        let mut config = synapse_common::config::worker::WorkerConfig { enabled: true, ..Default::default() };
        config.replication.http.enabled = true;

        let error = worker_replication_secret_check(&config).expect_err("mounted without a secret must be fatal");
        assert!(error.contains("neither worker.replication.http.secret"), "{error}");
    }

    // `cfg`-gated rather than asserted inside: `cargo test --release` runs with
    // `debug_assertions` off, where the check below legitimately rejects the fixture.
    #[cfg(debug_assertions)]
    #[test]
    fn worker_replication_secret_check_does_not_use_strict_mode_in_debug_builds() {
        // Guards against inverting the build-mode switch: if this helper read
        // `cfg!(debug_assertions)` (or hard-coded `true`), the repository's own
        // fixtures would make every developer's server refuse to start. The tests
        // that prove the *release* branch rejects these values live next to
        // `validate_replication_http_secret` in `synapse-common` (they pass `strict`
        // explicitly, which is why the parameter exists).
        let mut config = synapse_common::config::worker::WorkerConfig { enabled: true, ..Default::default() };
        config.replication.http.enabled = true;
        config.replication.http.secret = Some("test_worker_secret".to_string());

        assert!(worker_replication_secret_check(&config).is_ok());
    }

    #[test]
    fn dehydrated_device_cleanup_uses_minimum_interval() {
        assert_eq!(
            dehydrated_device_cleanup_interval(60),
            Duration::from_secs(MIN_DEHYDRATED_DEVICE_CLEANUP_INTERVAL_SECS)
        );
    }

    #[test]
    fn dehydrated_device_cleanup_uses_background_interval_when_larger() {
        assert_eq!(dehydrated_device_cleanup_interval(900), Duration::from_secs(900));
    }

    #[test]
    fn global_maintenance_defaults_to_master_without_workers() {
        let config = synapse_common::config::worker::WorkerConfig::default();

        assert_eq!(global_maintenance_owner(&config), WorkerType::Master);
        assert!(should_run_global_maintenance(&config));
    }

    #[test]
    fn global_maintenance_prefers_background_worker_when_present() {
        let mut config = synapse_common::config::worker::WorkerConfig {
            enabled: true,
            instance_name: "background_worker".to_string(),
            ..Default::default()
        };
        config.instance_map.insert(
            "background_worker".to_string(),
            synapse_common::config::worker::InstanceLocationConfig {
                host: "127.0.0.1".to_string(),
                port: 8105,
                tls: false,
            },
        );

        assert_eq!(global_maintenance_owner(&config), WorkerType::Background);
        assert!(should_run_global_maintenance(&config));
    }

    #[test]
    fn master_skips_global_maintenance_when_background_worker_exists() {
        let mut config = synapse_common::config::worker::WorkerConfig {
            enabled: true,
            instance_name: "master".to_string(),
            ..Default::default()
        };
        config.instance_map.insert(
            "background_worker".to_string(),
            synapse_common::config::worker::InstanceLocationConfig {
                host: "127.0.0.1".to_string(),
                port: 8105,
                tls: false,
            },
        );

        assert_eq!(global_maintenance_owner(&config), WorkerType::Background);
        assert!(!should_run_global_maintenance(&config));
    }

    #[test]
    fn render_appservice_scheduler_prometheus_metrics_includes_expected_series() {
        let summary = AppserviceSchedulerTelemetrySummary {
            total_services: 2,
            scheduler_available_services: 2,
            services_in_backoff: 1,
            services_capacity_limited: 1,
            services_with_pending_transactions: 1,
            total_pending_events: 7,
            total_pending_transactions: 3,
            total_success_count: 9,
            total_failure_count: 2,
            total_backoff_count: 1,
            total_capacity_limited_count: 4,
            total_in_flight_count: 5,
        };

        let rendered = render_appservice_scheduler_prometheus_metrics(&summary);

        assert!(rendered.contains("synapse_appservice_scheduler_total_services 2"));
        assert!(rendered.contains("synapse_appservice_scheduler_backoff_services 1"));
        assert!(rendered.contains("synapse_appservice_scheduler_pending_events 7"));
        assert!(rendered.contains("synapse_appservice_scheduler_in_flight_count 5"));
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn render_appservice_scheduler_prometheus_metrics_reflects_recovery_summary() {
        #[cfg(test)]
        crate::test_exit_hook::ensure();
        let pool = prepare_shared_test_pool().await.expect("shared test pool should be available");
        let container = synapse_services::ServiceContainer::new_test_with_pool(pool.clone()).await;
        let manager = container.admin.modules.app_service_manager.clone();
        let scheduler = container.admin.modules.app_service_scheduler.clone();
        let storage = ApplicationServiceStorage::new(&pool);

        let failing_server = MockServer::start().await;
        Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&failing_server).await;
        Mock::given(method("PUT"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(1)
            .mount(&failing_server)
            .await;

        let healthy_txn_server = MockServer::start().await;
        Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&healthy_txn_server).await;

        let healthy_event_server = MockServer::start().await;
        Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&healthy_event_server).await;

        let scenario_id = current_timestamp_millis();
        let failing_as_id = format!("prometheus-recovery-failing-{scenario_id}");
        let healthy_txn_as_id = format!("prometheus-recovery-txn-{scenario_id}");
        let healthy_event_as_id = format!("prometheus-recovery-event-{scenario_id}");
        let healthy_event_room_id = format!("!prometheus-recovery-event-{scenario_id}:localhost");

        manager
            .register(RegisterApplicationServiceRequest {
                as_id: failing_as_id.clone(),
                url: failing_server.uri(),
                as_token: format!("as_token_{failing_as_id}"),
                hs_token: format!("hs_token_{failing_as_id}"),
                sender: "@bridge:localhost".to_string(),
                description: Some("prometheus transient failing bridge".to_string()),
                is_rate_limited: Some(false),
                protocols: None,
                namespaces: Some(serde_json::json!({
                    "users": [],
                    "aliases": [],
                    "rooms": [{"exclusive": true, "regex": format!("^!prometheus-recovery-failing-{scenario_id}.*:localhost$")}]
                })),
                api_key: None,
                config: None,
            })
            .await
            .expect("failing appservice registration should succeed");

        manager
            .register(RegisterApplicationServiceRequest {
                as_id: healthy_txn_as_id.clone(),
                url: healthy_txn_server.uri(),
                as_token: format!("as_token_{healthy_txn_as_id}"),
                hs_token: format!("hs_token_{healthy_txn_as_id}"),
                sender: "@bridge:localhost".to_string(),
                description: Some("prometheus healthy txn bridge".to_string()),
                is_rate_limited: Some(false),
                protocols: None,
                namespaces: Some(serde_json::json!({
                    "users": [],
                    "aliases": [],
                    "rooms": [{"exclusive": true, "regex": format!("^!prometheus-recovery-txn-{scenario_id}.*:localhost$")}]
                })),
                api_key: None,
                config: None,
            })
            .await
            .expect("healthy transaction appservice registration should succeed");

        manager
            .register(RegisterApplicationServiceRequest {
                as_id: healthy_event_as_id.clone(),
                url: healthy_event_server.uri(),
                as_token: format!("as_token_{healthy_event_as_id}"),
                hs_token: format!("hs_token_{healthy_event_as_id}"),
                sender: "@bridge:localhost".to_string(),
                description: Some("prometheus healthy event bridge".to_string()),
                is_rate_limited: Some(false),
                protocols: None,
                namespaces: Some(serde_json::json!({
                    "users": [],
                    "aliases": [],
                    "rooms": [{"exclusive": true, "regex": format!("^!prometheus-recovery-event-{scenario_id}.*:localhost$")}]
                })),
                api_key: None,
                config: None,
            })
            .await
            .expect("healthy event appservice registration should succeed");

        storage
            .create_transaction(
                &failing_as_id,
                &format!("prometheus-recovery-failing-{scenario_id}"),
                &[serde_json::json!({"type": "m.room.message", "content": {"body": "fail once"}})],
            )
            .await
            .expect("failing pending transaction should be created");
        storage
            .create_transaction(
                &healthy_txn_as_id,
                &format!("prometheus-recovery-healthy-{scenario_id}"),
                &[serde_json::json!({"type": "m.room.message", "content": {"body": "healthy"}})],
            )
            .await
            .expect("healthy pending transaction should be created");

        for event_index in 0..60 {
            manager
                .push_event(
                    &healthy_event_as_id,
                    &healthy_event_room_id,
                    "m.room.message",
                    "@bridge:localhost",
                    serde_json::json!({"msgtype": "m.text", "body": format!("prometheus-event-{event_index}")}),
                    None,
                )
                .await
                .expect("healthy event enqueue should succeed");
        }

        scheduler.run_once().await.expect("prometheus recovery tick one should complete");
        scheduler.run_once().await.expect("prometheus recovery tick two should complete");
        tokio::time::sleep(std::time::Duration::from_millis(4_200)).await;
        scheduler.run_once().await.expect("prometheus recovery tick three should complete");

        let appservice_statistics = manager.get_statistics().await.expect("scheduler statistics should load");
        let summary = summarize_appservice_scheduler_metrics(&appservice_statistics);
        let rendered = render_appservice_scheduler_prometheus_metrics(&summary);

        assert_eq!(summary.total_services, 3);
        assert_eq!(summary.scheduler_available_services, 3);
        assert_eq!(summary.services_in_backoff, 0);
        assert_eq!(summary.services_with_pending_transactions, 0);
        assert_eq!(summary.total_pending_events, 0);
        assert_eq!(summary.total_pending_transactions, 0);
        assert_eq!(summary.total_success_count, 3);

        assert!(rendered.contains("synapse_appservice_scheduler_total_services 3"));
        assert!(rendered.contains("synapse_appservice_scheduler_available_services 3"));
        assert!(rendered.contains("synapse_appservice_scheduler_backoff_services 0"));
        assert!(rendered.contains("synapse_appservice_scheduler_pending_events 0"));
        assert!(rendered.contains("synapse_appservice_scheduler_pending_transactions 0"));
        assert!(rendered.contains("synapse_appservice_scheduler_success_count 3"));
    }
}

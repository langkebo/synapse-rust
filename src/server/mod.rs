use crate::common::config::Config;
use crate::common::rate_limit_config::{start_config_watcher, RateLimitConfigFile, RateLimitConfigManager};
use crate::tasks::{ScheduledTasks, TaskMetricsCollector};
use crate::web::middleware::{
    check_cors_security, log_cors_security_report, set_bind_address, set_config_allowed_origins,
    set_trust_forwarded_headers, validate_bind_address_for_dev_mode,
};
use crate::web::routes::telemetry::{summarize_appservice_scheduler_metrics, AppserviceSchedulerTelemetrySummary};
use crate::web::AppState;
use crate::worker::topology_validator::{
    current_instance_worker_type, global_maintenance_owner, should_run_global_maintenance,
};
use axum::{response::IntoResponse, routing::get, Router};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tokio::signal;

use synapse_storage::*;

mod database;
mod router;
mod services;
pub mod telemetry;

const MIN_DEHYDRATED_DEVICE_CLEANUP_INTERVAL_SECS: u64 = 300;

// --- Tuning constants ---

/// Interval (seconds) between background maintenance task ticks.
const BACKGROUND_TASK_INTERVAL_SECS: u64 = 60;

/// Minimum interval (seconds) between background task executions to prevent
/// tight loops when a task completes quickly.
const MIN_BACKGROUND_INTERVAL_SECS: u64 = 10;

/// Capacity of the tokio broadcast channel used for graceful shutdown signaling.
const SHUTDOWN_BROADCAST_CAPACITY: usize = 3;

/// Maximum retries for federation destination connection attempts
/// before marking a destination as unreachable.
const FEDERATION_RETRY_MAX_COUNT: u64 = 5;

/// Timeout (seconds) for draining in-flight requests during graceful shutdown.
const DRAIN_TIMEOUT_SECS: u64 = 30;

/// Interval (seconds) between megolm session key cleanup runs.
const MEGOLM_CLEANUP_INTERVAL_SECS: u64 = 6 * 3600;

/// Interval (seconds) between event pruning runs.
const PRUNING_INTERVAL_SECS: u64 = 86400;

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

fn create_rate_limit_manager(config_path: &std::path::Path) -> Arc<RateLimitConfigManager> {
    let default_config = RateLimitConfigFile::default();
    Arc::new(RateLimitConfigManager::new(default_config, config_path.to_path_buf()))
}

pub struct SynapseServer {
    app_state: Arc<AppState>,
    router: Router,
    address: SocketAddr,
    federation_address: SocketAddr,
    media_path: std::path::PathBuf,
    scheduled_tasks: Arc<ScheduledTasks>,
    metrics_collector: Arc<TaskMetricsCollector>,
    _rate_limit_config_manager: Option<Arc<RateLimitConfigManager>>,
    _config_watcher_handle: Option<tokio::task::JoinHandle<()>>,
}

impl SynapseServer {
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

        let (services, cache, redis_pool_option) = services::build_service_container(&pool, &config).await?;

        // Startup topology validation — ensures worker configuration is consistent before proceeding
        {
            let validation = crate::worker::topology_validator::validate_worker_config(&config.worker);
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

        let (rate_limit_config_manager, config_watcher_handle) = if rate_limit_config_path.exists() {
            match RateLimitConfigManager::from_file(&rate_limit_config_path).await {
                Ok(manager) => {
                    let manager = Arc::new(manager);
                    let config = manager.get_config();
                    let handle = start_config_watcher(manager.clone(), config.reload_interval_seconds).await;
                    ::tracing::info!("Rate limit config loaded from {:?}", rate_limit_config_path);
                    (Some(manager), Some(handle))
                }
                Err(e) => {
                    ::tracing::warn!(
                        "Failed to load rate limit config from {:?}: {}. Using default config.",
                        rate_limit_config_path,
                        e
                    );
                    let manager = create_rate_limit_manager(&rate_limit_config_path);
                    (Some(manager), None)
                }
            }
        } else {
            ::tracing::info!(
                "Rate limit config file not found at {:?}. Using default config.",
                rate_limit_config_path.display()
            );
            let manager = create_rate_limit_manager(&rate_limit_config_path);
            (Some(manager), None)
        };

        let app_state = if let Some(ref manager) = rate_limit_config_manager {
            Arc::new((*app_state).clone().with_rate_limit_config(manager.clone()))
        } else {
            app_state
        };

        let scheduled_tasks =
            Arc::new(ScheduledTasks::new(Arc::new(Database::from_pool((*pool).clone(), redis_pool_option))));
        let metrics_collector = Arc::new(TaskMetricsCollector::new(scheduled_tasks.clone()));

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
            metrics_collector,
            _rate_limit_config_manager: rate_limit_config_manager,
            _config_watcher_handle: config_watcher_handle,
        })
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        ::tracing::info!("Starting Synapse Rust Matrix Server...");
        ::tracing::info!("Server name: {}", self.app_state.services.core.server_name);
        ::tracing::info!("Listening on (Client API): {}", self.address);
        ::tracing::info!("Listening on (Federation): {}", self.federation_address);
        if self.app_state.services.core.config.prometheus.enabled {
            ::tracing::info!(
                "Listening on (Prometheus): {}:{}{}",
                self.app_state.services.core.config.server.host,
                self.app_state.services.core.config.prometheus.port,
                self.app_state.services.core.config.prometheus.path
            );
        }
        ::tracing::info!("Media storage: {}", self.media_path.display());

        if let Err(e) = self.warmup().await {
            ::tracing::warn!("Warmup encountered minor errors: {}", e);
        }

        let worker_config = &self.app_state.services.core.config.worker;
        let current_worker_type = current_instance_worker_type(worker_config);
        let maintenance_owner = global_maintenance_owner(worker_config);
        let maintenance_runtime_enabled = global_maintenance_tasks_enabled();
        let run_global_maintenance = should_run_global_maintenance(worker_config) && maintenance_runtime_enabled;

        ::tracing::info!(
            worker_type = current_worker_type.as_str(),
            maintenance_owner = maintenance_owner.as_str(),
            maintenance_runtime_enabled,
            run_global_maintenance,
            "Evaluated global maintenance task ownership"
        );

        if run_global_maintenance {
            self.app_state.services.federation.key_rotation_manager.start_auto_rotation().await;

            ::tracing::info!("Starting scheduled database monitoring and maintenance tasks...");
            self.scheduled_tasks.start_all();
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
        let prometheus_config = self.app_state.services.core.config.prometheus.clone();
        let prometheus_listener = if prometheus_config.enabled {
            Some(
                tokio::net::TcpListener::bind(format!(
                    "{}:{}",
                    self.app_state.services.core.config.server.host, prometheus_config.port
                ))
                .await?,
            )
        } else {
            None
        };

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

        if run_global_maintenance {
            let bg_service = self.app_state.services.admin.modules.background_update_service.clone();
            let retention_service = self.app_state.services.admin.modules.retention_service.clone();
            let media_service = self.app_state.services.core.media_service.clone();
            let event_broadcaster = self.app_state.services.core.event_broadcaster.clone();
            let remote_media_lifetime = self.app_state.services.core.config.server.remote_media_lifetime;
            let local_media_lifetime = self.app_state.services.core.config.server.local_media_lifetime;
            let mut media_cleanup_counter: u64 = 0;
            let mut federation_retry_counter: u64 = 0;
            tokio::spawn(async move {
                let mut interval =
                    tokio::time::interval(tokio::time::Duration::from_secs(BACKGROUND_TASK_INTERVAL_SECS));
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
                                    let cutoff_ts = chrono::Utc::now().timestamp_millis()
                                        - (remote_media_lifetime as i64 * 1000);
                                    if let Err(e) = media_service.purge_media_cache(cutoff_ts).await {
                                        ::tracing::warn!("Remote media cleanup failed: {}", e);
                                    }
                                }
                                if local_media_lifetime > 0 {
                                    let cutoff_ts = chrono::Utc::now().timestamp_millis()
                                        - (local_media_lifetime as i64 * 1000);
                                    if let Err(e) = media_service.purge_media_cache(cutoff_ts).await {
                                        ::tracing::warn!("Local media cleanup failed: {}", e);
                                    }
                                }
                            }
                            federation_retry_counter += 1;
                            if federation_retry_counter >= FEDERATION_RETRY_MAX_COUNT {
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
            tokio::spawn(async move {
                let mut interval_timer =
                    tokio::time::interval(tokio::time::Duration::from_secs(MEGOLM_CLEANUP_INTERVAL_SECS));
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
            tokio::spawn(async move {
                let mut interval_timer = tokio::time::interval(Duration::from_secs(PRUNING_INTERVAL_SECS));
                interval_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                interval_timer.tick().await; // skip immediate tick after startup

                loop {
                    tokio::select! {
                        _ = interval_timer.tick() => {
                            prune_step!("device list changes", synapse_storage::pruning::prune_old_device_list_changes(&pruning_pool, synapse_storage::pruning::DEVICE_LIST_CHANGES_RETENTION_DAYS));

                            prune_step!("presence", synapse_storage::pruning::prune_expired_presence(&pruning_pool));

                            prune_step!("one-time keys", synapse_storage::pruning::prune_expired_one_time_keys(&pruning_pool));

                            // Extended pruning for additional append-only
                            // tables that accumulate without bound on long-running
                            // instances.
                            prune_step!("to-device transactions", synapse_storage::pruning::prune_old_to_device_transactions(&pruning_pool));

                            prune_step!("token blacklist", synapse_storage::pruning::prune_expired_token_blacklist(&pruning_pool));

                            prune_step!("federation queue", synapse_storage::pruning::prune_old_federation_queue(&pruning_pool));
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
            let metrics_state = PrometheusMetricsState {
                metrics: self.app_state.services.core.metrics.clone(),
                app_service_manager: self.app_state.services.admin.modules.app_service_manager.clone(),
            };
            let prometheus_path = prometheus_config.path.clone();
            let prometheus_router =
                Router::new().route(&prometheus_path, get(render_prometheus_metrics)).with_state(metrics_state);

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

        ::tracing::info!("All servers started successfully");

        // Spawn signal handler for graceful shutdown (ctrl_c / SIGTERM).
        let shutdown_tx_signal = shutdown_tx.clone();
        let worker_instance = std::env::var("WORKER_INSTANCE_NAME").unwrap_or_else(|_| "master".to_string());
        let start_ts = chrono::Utc::now().timestamp_millis();
        tokio::spawn(async move {
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
            let uptime_secs = (chrono::Utc::now().timestamp_millis() - start_ts) / 1000;
            ::tracing::warn!(
                target: "shutdown",
                signal = %sig,
                worker_instance = %worker_instance,
                uptime_secs = %uptime_secs,
                "Shutdown signal received — draining listeners"
            );
            let _ = shutdown_tx_signal.send(());
        });

        // Wait for all listeners to drain, with a hard 30s cap to prevent
        // long-polling endpoints (e.g. /sync with 90s+ timeout) from blocking
        // rolling updates indefinitely.
        let drain_timeout = Duration::from_secs(DRAIN_TIMEOUT_SECS);
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

        ::tracing::info!("Servers shutdown complete");

        // Pre-exit logging for key worker types: capture queue state, restart count, and exit reason.
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

    pub fn metrics_collector(&self) -> &Arc<TaskMetricsCollector> {
        &self.metrics_collector
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

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "test-utils")]
    use crate::test_utils::prepare_shared_test_pool;
    use crate::worker::types::WorkerType;
    #[cfg(feature = "test-utils")]
    use synapse_storage::application_service::{ApplicationServiceStorage, RegisterApplicationServiceRequest};
    #[cfg(feature = "test-utils")]
    use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

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

        let scenario_id = chrono::Utc::now().timestamp_millis();
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

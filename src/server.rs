use axum::{response::IntoResponse, routing::get, Router};
use deadpool_redis::Pool as RedisPool;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use crate::cache::*;
use crate::common::config::Config;
use crate::common::rate_limit_config::{start_config_watcher, RateLimitConfigFile, RateLimitConfigManager};
use crate::services::*;
use crate::storage::schema_health_check::run_schema_health_check;
use crate::storage::*;
use crate::tasks::{ScheduledTasks, TaskMetricsCollector};
use crate::web::middleware::{
    check_cors_security, log_cors_security_report, request_debug_middleware, request_timeout_middleware,
    set_bind_address, set_config_allowed_origins, set_trust_forwarded_headers, validate_bind_address_for_dev_mode,
};
use crate::web::routes::create_router;
use crate::web::AppState;

const DEFAULT_MAX_LIFETIME: Duration = Duration::from_secs(1800);
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(600);
const MIN_DEHYDRATED_DEVICE_CLEANUP_INTERVAL_SECS: u64 = 300;

fn dehydrated_device_cleanup_interval(configured_interval_secs: u64) -> Duration {
    Duration::from_secs(configured_interval_secs.max(MIN_DEHYDRATED_DEVICE_CLEANUP_INTERVAL_SECS))
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

use crate::common::task_queue::RedisTaskQueue;

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

        let pool_options = PgPoolOptions::new()
            .max_connections(config.database.max_size)
            .min_connections(config.database.min_idle.unwrap_or(5))
            .acquire_timeout(Duration::from_secs(config.database.connection_timeout))
            .max_lifetime(DEFAULT_MAX_LIFETIME)
            .idle_timeout(DEFAULT_IDLE_TIMEOUT)
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    sqlx::query!("SET statement_timeout = '30s'").execute(&mut *conn).await?;
                    sqlx::query!("SET lock_timeout = '10s'").execute(&mut *conn).await?;
                    sqlx::query!("SET idle_in_transaction_session_timeout = '60s'").execute(&mut *conn).await?;
                    Ok(())
                })
            })
            .test_before_acquire(true);

        ::tracing::info!("Connecting to database with optimized pool settings...");
        ::tracing::info!("  Max connections: {}", config.database.max_size);
        ::tracing::info!("  Min idle connections: {:?}", config.database.min_idle);
        ::tracing::info!("  Connection timeout: {}s", config.database.connection_timeout);

        let database_url = config.database_url();
        let pool = pool_options.connect(&database_url).await?;
        let pool = Arc::new(pool);

        // 先执行运行时数据库初始化，确保所有表存在
        let runtime_db_init_enabled = std::env::var("SYNAPSE_ENABLE_RUNTIME_DB_INIT")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false);
        let skip_db_init = std::env::var("SYNAPSE_SKIP_DB_INIT")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false);
        if !runtime_db_init_enabled || skip_db_init {
            ::tracing::info!(
                "Runtime database initialization disabled; use docker/db_migrate.sh and db-migration-gate.yml as the migration source of truth"
            );
        } else {
            let db_init_service = DatabaseInitService::new(pool.clone());
            db_init_service.initialize().await?;
        }

        // 运行数据库 Schema 健康检查（在运行时初始化之后）
        let skip_schema_check = std::env::var("SYNAPSE_SKIP_SCHEMA_CHECK").unwrap_or_default().to_lowercase() == "true";

        if skip_schema_check {
            ::tracing::warn!("⚠️  Skipping database schema health check (SYNAPSE_SKIP_SCHEMA_CHECK=true)");
        } else {
            ::tracing::info!("Running database schema health check...");
            match run_schema_health_check(&pool, false).await {
                Ok(result) => {
                    if result.passed {
                        ::tracing::info!("✅ Database schema validation PASSED");
                    } else {
                        ::tracing::error!("❌ Database schema validation FAILED");
                        if !result.missing_tables.is_empty() {
                            ::tracing::error!("  Missing tables: {:?}", result.missing_tables);
                        }
                        if !result.missing_columns.is_empty() {
                            ::tracing::error!("  Missing columns: {:?}", result.missing_columns);
                        }
                        if !result.repaired_indexes.is_empty() {
                            ::tracing::info!("  Repaired indexes: {:?}", result.repaired_indexes);
                        }
                        // 如果有严重问题（缺少表或列），给出可执行的修复指引后退出
                        if !result.missing_tables.is_empty() || !result.missing_columns.is_empty() {
                            ::tracing::error!(
                                "💡 To fix: run pending migrations against your database, e.g.\n   \
                                 DATABASE_URL=\"postgresql://USER:PASS@HOST:PORT/DBNAME\" \\\n   \
                                 bash docker/db_migrate.sh migrate\n   \
                                 If you understand the risk and want to start anyway, set \
                                 SYNAPSE_SKIP_SCHEMA_CHECK=true (NOT recommended for production)."
                            );
                            return Err("Database schema validation failed: missing critical tables or columns. \
                                 Run `docker/db_migrate.sh migrate` against the configured database \
                                 (or set SYNAPSE_SKIP_SCHEMA_CHECK=true to bypass this check)."
                                .into());
                        }
                    }
                    if !result.warnings.is_empty() {
                        ::tracing::warn!("Schema warnings (non-critical): {:?}", result.warnings);
                    }
                }
                Err(e) => {
                    ::tracing::error!("Failed to run schema health check: {}", e);
                    // 非致命错误，继续启动
                }
            }
        }

        let mut task_queue: Option<Arc<RedisTaskQueue>> = None;
        let mut redis_pool_option: Option<RedisPool> = None;

        let cache = if config.redis.enabled {
            ::tracing::info!("Redis enabled. Connecting to: {}:{}", config.redis.host, config.redis.port);

            let conn_str = config.redis.connection_url();
            let redis_cfg = deadpool_redis::Config::from_url(&conn_str);
            let redis_pool = redis_cfg.create_pool(Some(deadpool_redis::Runtime::Tokio1))?;
            redis_pool_option = Some(redis_pool.clone());

            ::tracing::info!("Redis pool created.");

            // Startup health check: verify Redis connectivity with a ping
            match redis_pool.get().await {
                Ok(mut conn) => {
                    let ping: redis::RedisResult<String> = redis::cmd("PING").query_async(&mut *conn).await;
                    match ping {
                        Ok(_) => ::tracing::info!("Redis connectivity verified (PING OK)."),
                        Err(e) => ::tracing::warn!("Redis PING failed: {}. Service may be degraded.", e),
                    }
                }
                Err(e) => {
                    ::tracing::warn!("Failed to acquire Redis connection from pool: {}. Service may be degraded.", e);
                }
            }

            let tq = RedisTaskQueue::from_pool(redis_pool.clone());
            task_queue = Some(Arc::new(tq));

            let cache = Arc::new(CacheManager::with_redis_pool_and_url(redis_pool, &CacheConfig::default(), &conn_str));

            if let Err(e) = cache.start_invalidation_subscriber() {
                ::tracing::warn!("Failed to start cache invalidation subscriber: {}", e);
            } else {
                ::tracing::info!("Cache invalidation subscriber started successfully");
            }

            cache
        } else {
            ::tracing::warn!(
                "Redis disabled. Using local in-memory cache. \
                 Rate limiting will use per-process in-memory token buckets, \
                 which are NOT shared across workers. For multi-worker deployments, \
                 enable Redis to ensure consistent rate limiting."
            );
            Arc::new(CacheManager::new(&CacheConfig::default()))
        };

        let services = ServiceContainer::new(&pool, cache.clone(), config.clone(), task_queue).await;
        let app_state = Arc::new(AppState::new(services, cache));

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
                    let default_config = RateLimitConfigFile::default();
                    let manager = Arc::new(RateLimitConfigManager::new(default_config, rate_limit_config_path));
                    (Some(manager), None)
                }
            }
        } else {
            ::tracing::info!(
                "Rate limit config file not found at {:?}. Using default config.",
                rate_limit_config_path.display()
            );
            let default_config = RateLimitConfigFile::default();
            let manager = Arc::new(RateLimitConfigManager::new(default_config, rate_limit_config_path));
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
        let media_path = std::path::PathBuf::from("/app/data/media");

        let router = create_router((*app_state).clone())
            .layer(RequestBodyLimitLayer::new(config.server.max_upload_size as usize))
            .layer(axum::middleware::from_fn(request_debug_middleware))
            .layer(axum::middleware::from_fn(request_timeout_middleware))
            .layer(TraceLayer::new_for_http());

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
        ::tracing::info!("Server name: {}", self.app_state.services.server_name);
        ::tracing::info!("Listening on (Client API): {}", self.address);
        ::tracing::info!("Listening on (Federation): {}", self.federation_address);
        if self.app_state.services.config.prometheus.enabled {
            ::tracing::info!(
                "Listening on (Prometheus): {}:{}{}",
                self.app_state.services.config.server.host,
                self.app_state.services.config.prometheus.port,
                self.app_state.services.config.prometheus.path
            );
        }
        ::tracing::info!("Media storage: {}", self.media_path.display());

        if let Err(e) = self.warmup().await {
            ::tracing::warn!("Warmup encountered minor errors: {}", e);
        }

        self.app_state.services.federation.key_rotation_manager.start_auto_rotation().await;

        ::tracing::info!("Starting scheduled database monitoring and maintenance tasks...");
        self.scheduled_tasks.start_all();

        #[cfg(feature = "beacons")]
        let beacon_service = self.app_state.services.rooms.beacon_service.clone();
        let retention_service = self.app_state.services.admin.retention_service.clone();
        let retention_config = self.app_state.services.config.retention.clone();
        let background_tasks_interval = self.app_state.services.config.server.background_tasks_interval.max(10);
        let dehydrated_cleanup_interval_secs =
            self.app_state.services.config.server.dehydrated_device_cleanup_interval_secs;
        let lifecycle_interval_secs = if retention_config.lifecycle_cleanup_enabled {
            retention_config.lifecycle_cleanup_interval_secs.max(background_tasks_interval)
        } else {
            background_tasks_interval
        };
        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(Duration::from_secs(lifecycle_interval_secs));
            interval_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval_timer.tick().await;
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
            }
        });

        let router = self.router.clone();
        let fed_router = self.router.clone();
        let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(3);

        let client_listener = tokio::net::TcpListener::bind(self.address).await?;
        let federation_listener = tokio::net::TcpListener::bind(self.federation_address).await?;
        let prometheus_config = self.app_state.services.config.prometheus.clone();
        let prometheus_listener = if prometheus_config.enabled {
            Some(
                tokio::net::TcpListener::bind(format!(
                    "{}:{}",
                    self.app_state.services.config.server.host, prometheus_config.port
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

        {
            let bg_service = self.app_state.services.admin.background_update_service.clone();
            let retention_service = self.app_state.services.admin.retention_service.clone();
            let media_service = self.app_state.services.media_service.clone();
            let event_broadcaster = self.app_state.services.event_broadcaster.clone();
            let remote_media_lifetime = self.app_state.services.config.server.remote_media_lifetime;
            let local_media_lifetime = self.app_state.services.config.server.local_media_lifetime;
            let mut media_cleanup_counter: u64 = 0;
            let mut federation_retry_counter: u64 = 0;
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
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
                            if federation_retry_counter >= 5 {
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

        {
            let dehydrated_service = self.app_state.services.e2ee.dehydrated_device_service.clone();
            let cleanup_interval = dehydrated_device_cleanup_interval(dehydrated_cleanup_interval_secs);
            let server_metrics = self.app_state.services.server_metrics.clone();
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
            let metrics = self.app_state.services.metrics.clone();
            let prometheus_path = prometheus_config.path.clone();
            let prometheus_router =
                Router::new().route(&prometheus_path, get(render_prometheus_metrics)).with_state(metrics);

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

        client_rx.await.ok();
        fed_rx.await.ok();
        prom_rx.await.ok();

        ::tracing::info!("Servers shutdown complete");
        Ok(())
    }

    async fn warmup(&self) -> Result<(), Box<dyn std::error::Error>> {
        let pool = &self.app_state.services.user_storage.pool;

        ::tracing::info!("Performing system warmup...");

        sqlx::query_scalar!("SELECT 1 AS health_check").fetch_one(&**pool).await?;

        let _ = sqlx::query_scalar!("SELECT count(*) FROM users").fetch_one(&**pool).await?;

        #[cfg(feature = "saml-sso")]
        {
            if let Err(e) = self.app_state.services.saml_service.hydrate_runtime_overrides().await {
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
    axum::extract::State(metrics): axum::extract::State<Arc<crate::common::metrics::MetricsCollector>>,
) -> impl IntoResponse {
    ([(http::header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")], metrics.to_prometheus_format())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

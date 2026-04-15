use axum::{response::IntoResponse, routing::get, Router};
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tower_http::cors::{Any, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use crate::cache::*;
use crate::common::config::Config;
use crate::common::rate_limit_config::{
    start_config_watcher, RateLimitConfigFile, RateLimitConfigManager,
};
use crate::services::*;
use crate::storage::schema_health_check::run_schema_health_check;
use crate::storage::*;
use crate::tasks::{ScheduledTasks, TaskMetricsCollector};
use crate::web::middleware::{
    check_cors_security, log_cors_security_report, panic_catcher_middleware,
    request_timeout_middleware, validate_bind_address_for_dev_mode,
};
use crate::web::routes::create_router;
use crate::web::AppState;

const DEFAULT_MAX_LIFETIME: Duration = Duration::from_secs(1800);
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(600);

pub struct SynapseServer {
    app_state: Arc<AppState>,
    router: Router,
    address: SocketAddr,
    federation_address: SocketAddr,
    media_path: std::path::PathBuf,
    scheduled_tasks: Arc<ScheduledTasks>,
    metrics_collector: Arc<TaskMetricsCollector>,
    rate_limit_config_manager: Option<Arc<RateLimitConfigManager>>,
    config_watcher_handle: Option<tokio::task::JoinHandle<()>>,
}

use crate::common::task_queue::RedisTaskQueue;

impl SynapseServer {
    pub async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
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
            .test_before_acquire(true);

        ::tracing::info!("Connecting to database with optimized pool settings...");
        ::tracing::info!("  Max connections: {}", config.database.max_size);
        ::tracing::info!("  Min idle connections: {:?}", config.database.min_idle);
        ::tracing::info!(
            "  Connection timeout: {}s",
            config.database.connection_timeout
        );

        let database_url = config.database_url();
        let pool = pool_options.connect(&database_url).await?;
        let pool = Arc::new(pool);

        // 运行数据库 Schema 健康检查
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
                    // 如果有严重问题（缺少表或列），退出
                    if !result.missing_tables.is_empty() || !result.missing_columns.is_empty() {
                        return Err(
                            "Database schema validation failed: missing critical tables or columns"
                                .into(),
                        );
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

        let mut task_queue: Option<Arc<RedisTaskQueue>> = None;

        let cache = if config.redis.enabled {
            ::tracing::info!(
                "Redis enabled. Connecting to: {}:{}",
                config.redis.host,
                config.redis.port
            );

            let conn_str = format!("redis://{}:{}", config.redis.host, config.redis.port);
            let redis_cfg = deadpool_redis::Config::from_url(&conn_str);
            let redis_pool = redis_cfg.create_pool(Some(deadpool_redis::Runtime::Tokio1))?;

            ::tracing::info!("Redis pool created.");

            let tq = RedisTaskQueue::from_pool(redis_pool.clone());
            task_queue = Some(Arc::new(tq));

            let cache = Arc::new(CacheManager::with_redis_pool_and_url(
                redis_pool,
                CacheConfig::default(),
                &conn_str,
            ));

            if let Err(e) = cache.start_invalidation_subscriber().await {
                ::tracing::warn!("Failed to start cache invalidation subscriber: {}", e);
            } else {
                ::tracing::info!("Cache invalidation subscriber started successfully");
            }

            cache
        } else {
            ::tracing::info!("Redis disabled. Using local in-memory cache.");
            Arc::new(CacheManager::new(CacheConfig::default()))
        };

        let services = ServiceContainer::new(&pool, cache.clone(), config.clone(), task_queue);
        let app_state = Arc::new(AppState::new(services, cache));

        let rate_limit_config_path = std::path::PathBuf::from(
            std::env::var("RATE_LIMIT_CONFIG_PATH")
                .unwrap_or_else(|_| "/app/config/rate_limit.yaml".to_string()),
        );

        let (rate_limit_config_manager, config_watcher_handle) = if rate_limit_config_path.exists()
        {
            match RateLimitConfigManager::from_file(&rate_limit_config_path).await {
                Ok(manager) => {
                    let manager = Arc::new(manager);
                    let config = manager.get_config();
                    let handle =
                        start_config_watcher(manager.clone(), config.reload_interval_seconds).await;
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
                    let manager = Arc::new(RateLimitConfigManager::new(
                        default_config,
                        rate_limit_config_path,
                    ));
                    (Some(manager), None)
                }
            }
        } else {
            ::tracing::info!(
                "Rate limit config file not found at {:?}. Using default config.",
                rate_limit_config_path.display()
            );
            let default_config = RateLimitConfigFile::default();
            let manager = Arc::new(RateLimitConfigManager::new(
                default_config,
                rate_limit_config_path,
            ));
            (Some(manager), None)
        };

        let app_state = if let Some(ref manager) = rate_limit_config_manager {
            Arc::new((*app_state).clone().with_rate_limit_config(manager.clone()))
        } else {
            app_state
        };

        let scheduled_tasks = Arc::new(ScheduledTasks::new(Arc::new(Database::from_pool(
            (*pool).clone(),
        ))));
        let metrics_collector = Arc::new(TaskMetricsCollector::new(scheduled_tasks.clone()));

        let address =
            format!("{}:{}", config.server.host, config.server.port).parse::<SocketAddr>()?;
        let federation_address = format!(
            "{}:{}",
            config.server.host, config.federation.federation_port
        )
        .parse::<SocketAddr>()?;
        let media_path = std::path::PathBuf::from("/app/data/media");

        let router = create_router((*app_state).clone())
            .layer(RequestBodyLimitLayer::new(
                config.server.max_upload_size as usize,
            ))
            .layer(axum::middleware::from_fn(panic_catcher_middleware))
            .layer(axum::middleware::from_fn(request_timeout_middleware))
            .layer({
                let cors = &config.cors;
                let mut layer = CorsLayer::new();

                if cors.allowed_origins.iter().any(|o| o == "*") {
                    layer = layer.allow_origin(Any);
                } else {
                    let origins: Vec<http::HeaderValue> = cors
                        .allowed_origins
                        .iter()
                        .filter_map(|o| http::HeaderValue::from_str(o).ok())
                        .collect();
                    if !origins.is_empty() {
                        layer = layer.allow_origin(origins);
                    } else {
                        layer = layer.allow_origin(Any);
                    }
                }

                if cors.allow_credentials && !cors.allowed_origins.iter().any(|o| o == "*") {
                    layer = layer.allow_credentials(true);
                } else {
                    layer = layer.allow_credentials(false);
                }
                if cors.allowed_methods.iter().any(|m| m == "*") {
                    layer = layer.allow_methods(Any);
                } else {
                    let methods: Vec<http::Method> = cors
                        .allowed_methods
                        .iter()
                        .filter_map(|m| http::Method::from_bytes(m.as_bytes()).ok())
                        .collect();
                    if !methods.is_empty() {
                        layer = layer.allow_methods(methods);
                    } else {
                        layer = layer.allow_methods(Any);
                    }
                }
                if cors.allowed_headers.iter().any(|h| h == "*") {
                    layer = layer.allow_headers(Any);
                } else {
                    let headers: Vec<http::HeaderName> = cors
                        .allowed_headers
                        .iter()
                        .filter_map(|h| http::HeaderName::from_str(h).ok())
                        .collect();
                    if !headers.is_empty() {
                        layer = layer.allow_headers(headers);
                    } else {
                        layer = layer.allow_headers(Any);
                    }
                }
                layer
            })
            .layer(TraceLayer::new_for_http());

        Ok(Self {
            app_state,
            router,
            address,
            federation_address,
            media_path,
            scheduled_tasks,
            metrics_collector,
            rate_limit_config_manager,
            config_watcher_handle,
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

        self.app_state
            .services
            .key_rotation_manager
            .start_auto_rotation()
            .await;

        ::tracing::info!("Starting scheduled database monitoring and maintenance tasks...");
        self.scheduled_tasks.start_all().await;

        let beacon_service = self.app_state.services.beacon_service.clone();
        let retention_service = self.app_state.services.retention_service.clone();
        let retention_config = self.app_state.services.config.retention.clone();
        let background_tasks_interval = self
            .app_state
            .services
            .config
            .server
            .background_tasks_interval
            .max(10);
        let lifecycle_interval_secs = if retention_config.lifecycle_cleanup_enabled {
            retention_config
                .lifecycle_cleanup_interval_secs
                .max(background_tasks_interval)
        } else {
            background_tasks_interval
        };
        tokio::spawn(async move {
            let mut interval_timer =
                tokio::time::interval(Duration::from_secs(lifecycle_interval_secs));
            interval_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval_timer.tick().await;
                if retention_config.lifecycle_cleanup_enabled {
                    retention_service
                        .run_data_lifecycle_cycle(&beacon_service, &retention_config)
                        .await;
                } else {
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

        tokio::spawn(async move {
            let _ = shutdown_tx;
            axum::serve(
                client_listener,
                router.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async move {
                shutdown_rx1.recv().await.ok();
            })
            .await
            .ok();
            let _ = client_tx.send(());
        });

        tokio::spawn(async move {
            axum::serve(
                federation_listener,
                fed_router.into_make_service_with_connect_info::<SocketAddr>(),
            )
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
            let prometheus_router = Router::new()
                .route(&prometheus_path, get(render_prometheus_metrics))
                .with_state(metrics);

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

        sqlx::query("SELECT 1").execute(&**pool).await?;

        let _ = sqlx::query("SELECT count(*) FROM users")
            .fetch_one(&**pool)
            .await?;

        ::tracing::info!("Warmup completed successfully.");
        Ok(())
    }

    pub fn metrics_collector(&self) -> &Arc<TaskMetricsCollector> {
        &self.metrics_collector
    }
}

async fn render_prometheus_metrics(
    axum::extract::State(metrics): axum::extract::State<
        Arc<crate::common::metrics::MetricsCollector>,
    >,
) -> impl IntoResponse {
    (
        [(
            http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        metrics.to_prometheus_format(),
    )
}

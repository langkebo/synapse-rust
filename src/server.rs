use axum::Router;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tower_http::cors::{Any, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

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
    validate_cors_config_for_production,
};
use crate::web::routes::create_router;
use crate::web::AppState;
use tracing::{error, warn};

const DEFAULT_MAX_LIFETIME: Duration = Duration::from_secs(1800);
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(600);

#[allow(dead_code)]
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

        if let Err(e) = validate_cors_config_for_production() {
            tracing::error!("CORS configuration validation failed: {}", e);
            return Err(e.into());
        }

        if let Err(e) = validate_bind_address_for_dev_mode(&config.server.host) {
            tracing::warn!("{}", e);
        }

        let pool_options = PgPoolOptions::new()
            .max_connections(config.database.max_size)
            .min_connections(config.database.min_idle.unwrap_or(5))
            .acquire_timeout(Duration::from_secs(config.database.connection_timeout))
            .max_lifetime(DEFAULT_MAX_LIFETIME)
            .idle_timeout(DEFAULT_IDLE_TIMEOUT)
            .test_before_acquire(true);

        info!("Connecting to database with optimized pool settings...");
        info!("  Max connections: {}", config.database.max_size);
        info!("  Min idle connections: {:?}", config.database.min_idle);
        info!(
            "  Connection timeout: {}s",
            config.database.connection_timeout
        );

        let database_url = config.database_url();
        let pool = pool_options.connect(&database_url).await?;
        let pool = Arc::new(pool);

        // 运行数据库 Schema 健康检查
        info!("Running database schema health check...");
        match run_schema_health_check(&pool, true).await {
            Ok(result) => {
                if result.passed {
                    info!("✅ Database schema validation PASSED");
                } else {
                    error!("❌ Database schema validation FAILED");
                    if !result.missing_tables.is_empty() {
                        error!("  Missing tables: {:?}", result.missing_tables);
                    }
                    if !result.missing_columns.is_empty() {
                        error!("  Missing columns: {:?}", result.missing_columns);
                    }
                    if !result.repaired_indexes.is_empty() {
                        info!("  Repaired indexes: {:?}", result.repaired_indexes);
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
                    warn!("Schema warnings (non-critical): {:?}", result.warnings);
                }
            }
            Err(e) => {
                error!("Failed to run schema health check: {}", e);
                // 非致命错误，继续启动
            }
        }

        // Initialize database using the new database initialization service
        let db_init_service = DatabaseInitService::new(pool.clone());
        db_init_service.initialize().await?;

        let mut task_queue: Option<Arc<RedisTaskQueue>> = None;

        let cache = if config.redis.enabled {
            info!(
                "Redis enabled. Connecting to: {}:{}",
                config.redis.host, config.redis.port
            );

            let conn_str = if let Some(ref password) = config.redis.password {
                format!(
                    "redis://:{}@{}:{}",
                    password, config.redis.host, config.redis.port
                )
            } else {
                format!("redis://{}:{}", config.redis.host, config.redis.port)
            };
            let redis_cfg = deadpool_redis::Config::from_url(&conn_str);
            let redis_pool = redis_cfg.create_pool(Some(deadpool_redis::Runtime::Tokio1))?;

            info!("Redis pool created.");

            let tq = RedisTaskQueue::from_pool(redis_pool.clone());
            task_queue = Some(Arc::new(tq));

            let cache = Arc::new(CacheManager::with_redis_pool_and_url(
                redis_pool,
                CacheConfig::default(),
                &conn_str,
            ));

            if let Err(e) = cache.start_invalidation_subscriber().await {
                tracing::warn!("Failed to start cache invalidation subscriber: {}", e);
            } else {
                info!("Cache invalidation subscriber started successfully");
            }

            cache
        } else {
            info!("Redis disabled. Using local in-memory cache.");
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
                    info!("Rate limit config loaded from {:?}", rate_limit_config_path);
                    (Some(manager), Some(handle))
                }
                Err(e) => {
                    tracing::warn!(
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
            info!(
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
        info!("Starting Synapse Rust Matrix Server...");
        info!("Server name: {}", self.app_state.services.server_name);
        info!("Client API listening on: {}", self.address);
        info!("Federation API listening on: {}", self.federation_address);
        info!("Media storage: {}", self.media_path.display());

        info!("Performing system warmup...");
        if let Err(e) = self.warmup().await {
            tracing::warn!("Warmup encountered minor errors: {}", e);
        }

        self.app_state
            .services
            .key_rotation_manager
            .start_auto_rotation()
            .await;

        info!("Starting scheduled database monitoring and maintenance tasks...");
        self.scheduled_tasks.start_all().await;

        let client_router = self.router.clone();
        let client_addr = self.address;
        let federation_router = self.create_federation_router().await?;
        let fed_addr = self.federation_address;

        let client_handle = tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(client_addr)
                .await
                .expect("Failed to bind client API port");
            info!("Client API server started on {}", client_addr);
            axum::serve(listener, client_router)
                .with_graceful_shutdown(async {
                    shutdown_signal().await;
                })
                .await
                .expect("Client API server error");
        });

        let federation_handle = tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(fed_addr)
                .await
                .expect("Failed to bind federation port");
            info!("Federation API server started on {}", fed_addr);
            axum::serve(listener, federation_router)
                .with_graceful_shutdown(async {
                    shutdown_signal().await;
                })
                .await
                .expect("Federation API server error");
        });

        tokio::select! {
            result = client_handle => {
                if let Err(e) = result {
                    error!("Client API server panicked: {}", e);
                }
            }
            result = federation_handle => {
                if let Err(e) = result {
                    error!("Federation API server panicked: {}", e);
                }
            }
        }

        Ok(())
    }

    async fn create_federation_router(&self) -> Result<Router, Box<dyn std::error::Error>> {
        use tower_http::trace::TraceLayer;

        let federation_router =
            create_router((*self.app_state).clone()).layer(TraceLayer::new_for_http());

        Ok(federation_router)
    }

    async fn warmup(&self) -> Result<(), Box<dyn std::error::Error>> {
        let pool = &self.app_state.services.user_storage.pool;

        // 1. Warmup DB connections with a simple query
        sqlx::query("SELECT 1").execute(&**pool).await?;

        // 2. Warmup common lookup tables (e.g., server version, active users count)
        let _ = sqlx::query("SELECT count(*) FROM users")
            .fetch_one(&**pool)
            .await?;

        // 3. Populate Redis/Cache with critical system configs if any
        // (Currently handled by ServiceContainer initialization, but could add specific ones)

        info!("Warmup completed successfully.");
        Ok(())
    }

    pub fn metrics_collector(&self) -> &Arc<TaskMetricsCollector> {
        &self.metrics_collector
    }
}

async fn shutdown_signal() {
    if let Err(e) = signal::ctrl_c().await {
        tracing::error!("Failed to install Ctrl+C handler: {}", e);
    }
    info!("Shutting down server...");
}

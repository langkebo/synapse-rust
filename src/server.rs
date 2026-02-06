use axum::Router;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::cache::*;
use crate::common::config::Config;
use crate::services::*;
use crate::storage::*;
use crate::tasks::{ScheduledTasks, TaskMetricsCollector};
use crate::web::routes::create_router;
use crate::web::AppState;

const DEFAULT_MAX_LIFETIME: Duration = Duration::from_secs(1800);
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(600);

pub struct SynapseServer {
    app_state: Arc<AppState>,
    router: Router,
    address: SocketAddr,
    media_path: std::path::PathBuf,
    scheduled_tasks: Arc<ScheduledTasks>,
    metrics_collector: Arc<TaskMetricsCollector>,
}

impl SynapseServer {
    pub async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
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

        // Initialize database using the new database initialization service
        let db_init_service = DatabaseInitService::new(pool.clone());
        db_init_service.initialize().await?;

        let cache = if config.redis.enabled {
            Arc::new(CacheManager::with_redis(&config.redis_url(), CacheConfig::default()).await?)
        } else {
            Arc::new(CacheManager::new(CacheConfig::default()))
        };
        let services = ServiceContainer::new(&pool, cache.clone(), config.clone());
        let app_state = Arc::new(AppState::new(services, cache.clone()));

        let scheduled_tasks = Arc::new(ScheduledTasks::new(Arc::new(Database::from_pool(
            (*pool).clone(),
        ))));
        let metrics_collector = Arc::new(TaskMetricsCollector::new(scheduled_tasks.clone()));

        let address =
            format!("{}:{}", config.server.host, config.server.port).parse::<SocketAddr>()?;
        let media_path = std::path::PathBuf::from("/app/data/media");

        let router = create_router((*app_state).clone())
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
            media_path,
            scheduled_tasks,
            metrics_collector,
        })
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting Synapse Rust Matrix Server...");
        info!("Server name: {}", self.app_state.services.server_name);
        info!("Listening on: {}", self.address);
        info!("Media storage: {}", self.media_path.display());

        // Performance Optimization: Warmup database and cache
        info!("Performing system warmup...");
        if let Err(e) = self.warmup().await {
            tracing::warn!("Warmup encountered minor errors: {}", e);
        }

        // Start key rotation scheduler
        self.app_state
            .services
            .key_rotation_manager
            .start_auto_rotation()
            .await;

        info!("Starting scheduled database monitoring and maintenance tasks...");
        self.scheduled_tasks.start_all().await;

        let listener = tokio::net::TcpListener::bind(self.address).await?;
        axum::serve(listener, self.router.clone())
            .with_graceful_shutdown(async {
                shutdown_signal().await;
            })
            .await?;

        Ok(())
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

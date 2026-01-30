use axum::{routing::get, Json, Router};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::cache::*;
use crate::services::*;
use crate::storage::*;
use crate::tasks::{ScheduledTasks, TaskMetricsCollector};
use crate::web::routes::create_admin_router;
use crate::web::routes::create_e2ee_router;
use crate::web::routes::create_federation_router;
use crate::web::routes::create_friend_router;
use crate::web::routes::create_key_backup_router;
use crate::web::routes::create_media_router;
use crate::web::routes::create_private_chat_router;
use crate::web::routes::create_router;
use crate::web::routes::create_voice_router;
use crate::web::AppState;

const DEFAULT_MAX_SIZE: u32 = 100;
const DEFAULT_MIN_IDLE: Option<u32> = Some(5);
const DEFAULT_CONNECTION_TIMEOUT: u64 = 30;
const DEFAULT_ACQUIRE_TIMEOUT: u64 = 30;
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
    pub async fn new(
        database_url: &str,
        server_name: &str,
        jwt_secret: &str,
        address: SocketAddr,
        media_path: std::path::PathBuf,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let pool_options = PgPoolOptions::new()
            .max_connections(DEFAULT_MAX_SIZE)
            .min_connections(DEFAULT_MIN_IDLE.unwrap_or(5))
            .acquire_timeout(Duration::from_secs(DEFAULT_ACQUIRE_TIMEOUT))
            .max_lifetime(DEFAULT_MAX_LIFETIME)
            .idle_timeout(DEFAULT_IDLE_TIMEOUT)
            .test_before_acquire(true);

        info!("Connecting to database with optimized pool settings...");
        info!("  Max connections: {}", DEFAULT_MAX_SIZE);
        info!("  Min idle connections: {:?}", DEFAULT_MIN_IDLE);
        info!("  Connection timeout: {}s", DEFAULT_CONNECTION_TIMEOUT);
        info!("  Acquire timeout: {}s", DEFAULT_ACQUIRE_TIMEOUT);
        info!("  Max lifetime: {}s", DEFAULT_MAX_LIFETIME.as_secs());
        info!("  Idle timeout: {}s", DEFAULT_IDLE_TIMEOUT.as_secs());

        let pool = pool_options.connect(database_url).await?;
        initialize_database(&pool).await?;
        let pool = Arc::new(pool);

        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        let services = ServiceContainer::new(&pool, cache.clone(), jwt_secret, server_name);
        let app_state = Arc::new(AppState::new(services, cache.clone()));

        let database = Arc::new(Database::new(database_url).await?);
        let scheduled_tasks = Arc::new(ScheduledTasks::new(database.clone()));
        let metrics_collector = Arc::new(TaskMetricsCollector::new(scheduled_tasks.clone()));

        let router = create_router((*app_state).clone())
            .merge(create_admin_router((*app_state).clone()))
            .merge(create_media_router(
                (*app_state).clone(),
                media_path.clone(),
            ))
            .merge(create_federation_router((*app_state).clone()))
            .merge(create_friend_router((*app_state).clone()))
            .merge(create_private_chat_router((*app_state).clone()))
            .merge(create_voice_router(
                (*app_state).clone(),
                std::path::PathBuf::from("/tmp/synapse_voice"),
            ))
            .merge(create_e2ee_router((*app_state).clone()))
            .merge(create_key_backup_router((*app_state).clone()))
            .route(
                "/{*path}",
                get(|| async { Json(json!({"errcode": "UNKNOWN", "error": "Unknown endpoint"})) }),
            )
            .layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any)
                    .allow_credentials(false),
            )
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
        info!("Starting scheduled database monitoring and maintenance tasks...");

        self.scheduled_tasks.start_all().await;

        let listener = tokio::net::TcpListener::bind(self.address).await?;
        let serve = axum::serve(listener, self.router.clone());
        let graceful = serve.with_graceful_shutdown(async {
            shutdown_signal().await;
        });
        let _ = graceful.await;

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

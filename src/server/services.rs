use deadpool_redis::Pool as RedisPool;
use std::sync::Arc;

use crate::cache::{CacheConfig, CacheManager};
use crate::common::config::Config;
use synapse_common::task_queue::RedisTaskQueue;
use synapse_services::ServiceContainer;

pub async fn build_service_container(
    pool: &Arc<sqlx::PgPool>,
    config: &Config,
) -> Result<(ServiceContainer, Arc<CacheManager>, Option<RedisPool>), Box<dyn std::error::Error>> {
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

    let services = ServiceContainer::new(pool, cache.clone(), config.clone(), task_queue).await;

    Ok((services, cache, redis_pool_option))
}

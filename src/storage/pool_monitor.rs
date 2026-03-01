use sqlx::postgres::{PgPool, PgPoolOptions};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

#[derive(Debug, Clone)]
pub struct DatabasePoolConfig {
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout: Duration,
    pub idle_timeout: Option<Duration>,
    pub max_lifetime: Option<Duration>,
    pub health_check_interval: Duration,
}

impl Default for DatabasePoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 20,
            min_connections: 5,
            acquire_timeout: Duration::from_secs(30),
            idle_timeout: Some(Duration::from_secs(600)),
            max_lifetime: Some(Duration::from_secs(1800)),
            health_check_interval: Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PoolHealthStatus {
    pub max_connections: u32,
    pub is_healthy: bool,
    pub last_check: Instant,
}

impl PoolHealthStatus {
    pub fn is_critical(&self) -> bool {
        false
    }

    pub fn is_warning(&self) -> bool {
        false
    }
}

pub struct DatabasePoolMonitor {
    pool: PgPool,
    config: DatabasePoolConfig,
    last_health_check: Arc<tokio::sync::RwLock<Option<PoolHealthStatus>>>,
}

impl DatabasePoolMonitor {
    pub fn new(pool: PgPool, config: DatabasePoolConfig) -> Self {
        Self {
            pool,
            config,
            last_health_check: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    pub async fn health_check(&self) -> Result<PoolHealthStatus, sqlx::Error> {
        let result = sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await;

        let is_healthy = result.is_ok();

        let health_status = PoolHealthStatus {
            max_connections: self.config.max_connections,
            is_healthy,
            last_check: Instant::now(),
        };

        let mut last_check = self.last_health_check.write().await;
        *last_check = Some(health_status.clone());

        if !is_healthy {
            tracing::error!(
                target: "database_pool",
                "Database pool health check failed"
            );
        }

        Ok(health_status)
    }

    pub async fn get_last_health_status(&self) -> Option<PoolHealthStatus> {
        let status = self.last_health_check.read().await;
        status.clone()
    }

    pub async fn test_connection(&self) -> Result<Duration, sqlx::Error> {
        let start = Instant::now();
        
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await?;

        Ok(start.elapsed())
    }

    pub async fn get_pool_stats(&self) -> PoolStats {
        PoolStats {
            max_connections: self.config.max_connections,
            is_closed: self.pool.is_closed(),
        }
    }

    pub fn start_health_check_task(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.config.health_check_interval);
            
            loop {
                interval.tick().await;
                
                if let Err(e) = self.health_check().await {
                    tracing::error!(
                        target: "database_pool",
                        error = %e,
                        "Database pool health check failed"
                    );
                }
            }
        })
    }
}

#[derive(Debug, Clone)]
pub struct PoolStats {
    pub max_connections: u32,
    pub is_closed: bool,
}

pub async fn create_pool_with_monitoring(
    database_url: &str,
    config: DatabasePoolConfig,
) -> Result<(PgPool, Arc<DatabasePoolMonitor>), sqlx::Error> {
    let mut pool_options = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(config.acquire_timeout);

    if let Some(idle_timeout) = config.idle_timeout {
        pool_options = pool_options.idle_timeout(idle_timeout);
    }

    if let Some(max_lifetime) = config.max_lifetime {
        pool_options = pool_options.max_lifetime(max_lifetime);
    }

    let pool = pool_options.connect(database_url).await?;

    let monitor = Arc::new(DatabasePoolMonitor::new(pool.clone(), config));

    Ok((pool, monitor))
}

pub struct QueryTimeoutConfig {
    pub default_timeout: Duration,
    pub long_query_timeout: Duration,
    pub transaction_timeout: Duration,
}

impl Default for QueryTimeoutConfig {
    fn default() -> Self {
        Self {
            default_timeout: Duration::from_secs(30),
            long_query_timeout: Duration::from_secs(120),
            transaction_timeout: Duration::from_secs(300),
        }
    }
}

pub fn set_query_timeout(timeout: Duration) -> String {
    format!("SET statement_timeout = {}ms", timeout.as_millis())
}

pub fn set_transaction_timeout(timeout: Duration) -> String {
    format!("SET idle_in_transaction_session_timeout = {}ms", timeout.as_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_config_default() {
        let config = DatabasePoolConfig::default();
        assert_eq!(config.max_connections, 20);
        assert_eq!(config.min_connections, 5);
        assert_eq!(config.acquire_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_pool_health_status() {
        let status = PoolHealthStatus {
            max_connections: 20,
            is_healthy: true,
            last_check: Instant::now(),
        };

        assert!(!status.is_warning());
        assert!(!status.is_critical());
    }

    #[test]
    fn test_query_timeout_config_default() {
        let config = QueryTimeoutConfig::default();
        assert_eq!(config.default_timeout, Duration::from_secs(30));
        assert_eq!(config.long_query_timeout, Duration::from_secs(120));
        assert_eq!(config.transaction_timeout, Duration::from_secs(300));
    }

    #[test]
    fn test_set_query_timeout() {
        let timeout = set_query_timeout(Duration::from_secs(10));
        assert_eq!(timeout, "SET statement_timeout = 10000ms");
    }

    #[test]
    fn test_set_transaction_timeout() {
        let timeout = set_transaction_timeout(Duration::from_secs(60));
        assert_eq!(timeout, "SET idle_in_transaction_session_timeout = 60000ms");
    }

    #[test]
    fn test_pool_stats() {
        let stats = PoolStats {
            max_connections: 20,
            is_closed: false,
        };

        assert_eq!(stats.max_connections, 20);
        assert!(!stats.is_closed);
    }

    #[test]
    fn test_pool_config_custom() {
        let config = DatabasePoolConfig {
            max_connections: 50,
            min_connections: 10,
            acquire_timeout: Duration::from_secs(60),
            idle_timeout: Some(Duration::from_secs(1200)),
            max_lifetime: Some(Duration::from_secs(3600)),
            health_check_interval: Duration::from_secs(60),
        };

        assert_eq!(config.max_connections, 50);
        assert_eq!(config.min_connections, 10);
        assert_eq!(config.acquire_timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_database_pool_monitor_fields() {
        let config = DatabasePoolConfig::default();
        assert!(config.max_connections > 0);
        assert!(config.min_connections > 0);
    }

    #[test]
    fn test_pool_stats_closed() {
        let stats = PoolStats {
            max_connections: 20,
            is_closed: true,
        };

        assert!(stats.is_closed);
    }
}

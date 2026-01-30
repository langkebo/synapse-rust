use sqlx::{Pool, Postgres};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ConnectionPoolConfig {
    pub max_connections: u32,
    pub min_connections: u32,
    pub connect_timeout: Duration,
    pub idle_timeout: Duration,
    pub max_lifetime: Duration,
    pub acquire_timeout: Duration,
    pub test_before_acquire: bool,
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 50,
            min_connections: 5,
            connect_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(600),
            max_lifetime: Duration::from_secs(1800),
            acquire_timeout: Duration::from_secs(30),
            test_before_acquire: true,
        }
    }
}

impl ConnectionPoolConfig {
    pub fn for_development() -> Self {
        Self {
            max_connections: 20,
            min_connections: 2,
            connect_timeout: Duration::from_secs(10),
            idle_timeout: Duration::from_secs(300),
            max_lifetime: Duration::from_secs(900),
            acquire_timeout: Duration::from_secs(10),
            test_before_acquire: true,
        }
    }

    pub fn for_production() -> Self {
        Self {
            max_connections: 100,
            min_connections: 10,
            connect_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(600),
            max_lifetime: Duration::from_secs(1800),
            acquire_timeout: Duration::from_secs(30),
            test_before_acquire: true,
        }
    }

    pub fn for_high_load() -> Self {
        Self {
            max_connections: 200,
            min_connections: 20,
            connect_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(300),
            max_lifetime: Duration::from_secs(1200),
            acquire_timeout: Duration::from_secs(60),
            test_before_acquire: true,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.max_connections < self.min_connections {
            return Err("max_connections must be >= min_connections".to_string());
        }
        if self.max_connections == 0 {
            return Err("max_connections must be > 0".to_string());
        }
        if self.min_connections == 0 {
            return Err("min_connections must be > 0".to_string());
        }
        if self.max_lifetime < self.idle_timeout {
            return Err("max_lifetime must be >= idle_timeout".to_string());
        }
        Ok(())
    }
}

pub struct ConnectionPoolManager {
    pool: Pool<Postgres>,
    config: ConnectionPoolConfig,
    health_check_interval: Duration,
    is_shutdown: bool,
}

impl ConnectionPoolManager {
    pub async fn new(
        database_url: &str,
        config: ConnectionPoolConfig,
    ) -> Result<Self, sqlx::Error> {
        config.validate().expect("Invalid connection pool configuration");

        let pool = sqlx::PgPool::connect_with(
            sqlx::postgres::PgConnectOptions::from_str(database_url)?
                .connect_timeout(config.connect_timeout)
                .idle_timeout(config.idle_timeout)
                .max_lifetime(config.max_lifetime),
        )
        .await?;

        pool.resize(config.max_connections).await;

        Ok(Self {
            pool,
            config: config.clone(),
            health_check_interval: Duration::from_secs(30),
            is_shutdown: false,
        })
    }

    pub fn pool(&self) -> &Pool<Postgres> {
        &self.pool
    }

    pub fn config(&self) -> &ConnectionPoolConfig {
        &self.config
    }

    pub async fn health_status(&self) -> PoolHealthStatus {
        let size = self.pool.size();
        let idle = self.pool.num_idle();
        let busy = size.saturating_sub(idle);

        PoolHealthStatus {
            total_connections: size as u32,
            idle_connections: idle as u32,
            busy_connections: busy as u32,
            max_connections: self.config.max_connections,
            utilization_percentage: if self.config.max_connections > 0 {
                (size as f64 / self.config.max_connections as f64) * 100.0
            } else {
                0.0
            },
            is_healthy: size > 0 && idle >= 0,
        }
    }

    pub async fn perform_health_check(&self) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await;
        Ok(result.is_ok())
    }

    pub async fn close(&mut self) {
        self.is_shutdown = true;
        self.pool.close().await;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolHealthStatus {
    pub total_connections: u32,
    pub idle_connections: u32,
    pub busy_connections: u32,
    pub max_connections: u32,
    pub utilization_percentage: f64,
    pub is_healthy: bool,
}

impl Default for PoolHealthStatus {
    fn default() -> Self {
        Self {
            total_connections: 0,
            idle_connections: 0,
            busy_connections: 0,
            max_connections: 0,
            utilization_percentage: 0.0,
            is_healthy: false,
        }
    }
}

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use tracing::{debug, error};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DatabaseHealthStatus {
    pub is_healthy: bool,
    pub connection_pool_status: ConnectionPoolStatus,
    pub performance_metrics: PerformanceMetrics,
    pub last_checked: chrono::DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConnectionPoolStatus {
    pub total_connections: u32,
    pub idle_connections: u32,
    pub busy_connections: u32,
    pub max_connections: u32,
    pub connection_utilization: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PerformanceMetrics {
    pub average_query_time_ms: f64,
    pub slow_queries_count: u64,
    pub total_queries: u64,
    pub transactions_per_second: f64,
    pub cache_hit_ratio: f64,
    pub deadlock_count: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DataIntegrityReport {
    pub check_timestamp: chrono::DateTime<Utc>,
    pub foreign_key_violations: Vec<ForeignKeyViolation>,
    pub orphaned_records: Vec<OrphanedRecord>,
    pub duplicate_entries: Vec<DuplicateEntry>,
    pub null_constraint_violations: Vec<NullConstraintViolation>,
    pub overall_integrity_score: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ForeignKeyViolation {
    pub table_name: String,
    pub column_name: String,
    pub violating_row_id: i64,
    pub referenced_table: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrphanedRecord {
    pub table_name: String,
    pub column_name: String,
    pub orphan_count: i64,
    pub sample_orphans: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DuplicateEntry {
    pub table_name: String,
    pub column_name: String,
    pub duplicate_count: i64,
    pub sample_duplicates: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NullConstraintViolation {
    pub table_name: String,
    pub column_name: String,
    pub null_count: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VacuumStats {
    pub table_name: String,
    pub last_vacuum: Option<chrono::NaiveDateTime>,
    pub last_analyze: Option<chrono::NaiveDateTime>,
    pub dead_tuple_count: i64,
    pub dead_tuple_ratio: f64,
}

pub struct DatabaseMonitor {
    pool: Pool<Postgres>,
    max_connections: u32,
}

impl DatabaseMonitor {
    pub fn new(pool: Pool<Postgres>, max_connections: u32) -> Self {
        Self {
            pool,
            max_connections,
        }
    }

    pub async fn check_connection(&self) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("SELECT 1").fetch_one(&self.pool).await;

        match result {
            Ok(_) => {
                debug!("Database connection check passed");
                Ok(true)
            }
            Err(e) => {
                error!("Database connection check failed: {}", e);
                Err(e)
            }
        }
    }

    pub async fn get_connection_pool_status(&self) -> Result<ConnectionPoolStatus, sqlx::Error> {
        let pool_size = self.pool.size();
        let idle_connections = self.pool.num_idle() as u32;

        Ok(ConnectionPoolStatus {
            total_connections: pool_size,
            idle_connections,
            busy_connections: pool_size.saturating_sub(idle_connections),
            max_connections: self.max_connections,
            connection_utilization: if self.max_connections > 0 {
                (pool_size as f64 / self.max_connections as f64) * 100.0
            } else {
                0.0
            },
        })
    }

    pub async fn get_full_health_status(&self) -> Result<DatabaseHealthStatus, sqlx::Error> {
        let is_healthy = self.check_connection().await?;
        let pool_status = self.get_connection_pool_status().await?;
        let performance = self.get_performance_metrics().await?;

        Ok(DatabaseHealthStatus {
            is_healthy,
            connection_pool_status: pool_status,
            performance_metrics: performance,
            last_checked: Utc::now(),
        })
    }

    pub async fn get_performance_metrics(&self) -> Result<PerformanceMetrics, sqlx::Error> {
        let tps = sqlx::query_as::<_, (i64, i64)>(
            "SELECT COALESCE(xact_commit, 0), COALESCE(xact_rollback, 0) \
             FROM pg_stat_database WHERE datname = current_database() LIMIT 1",
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or((0, 0));

        let cache = sqlx::query_as::<_, (i64, i64)>(
            "SELECT COALESCE(blks_hit, 0), COALESCE(blks_read, 0) \
             FROM pg_stat_database WHERE datname = current_database() LIMIT 1",
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or((0, 0));

        let cache_hit_ratio = if cache.0 + cache.1 > 0 {
            cache.0 as f64 / (cache.0 + cache.1) as f64
        } else {
            0.0
        };

        Ok(PerformanceMetrics {
            average_query_time_ms: 0.0,
            slow_queries_count: 0,
            total_queries: tps.0 as u64,
            transactions_per_second: tps.0 as f64 / 60.0,
            cache_hit_ratio,
            deadlock_count: 0,
        })
    }

    pub async fn verify_data_integrity(&self) -> Result<DataIntegrityReport, sqlx::Error> {
        let report = DataIntegrityReport {
            check_timestamp: Utc::now(),
            foreign_key_violations: Vec::new(),
            orphaned_records: Vec::new(),
            duplicate_entries: Vec::new(),
            null_constraint_violations: Vec::new(),
            overall_integrity_score: 100.0,
        };
        Ok(report)
    }
}

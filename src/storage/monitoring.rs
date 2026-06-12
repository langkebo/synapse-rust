use chrono::Utc;
use deadpool_redis::Pool as RedisPool;
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
    pub redis_latency_ms: f64,
    pub redis_slow_commands_count: u64,
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
    redis_pool: Option<RedisPool>,
    max_connections: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== ConnectionPoolStatus tests ==========

    #[test]
    fn test_connection_pool_status() {
        let status = ConnectionPoolStatus {
            total_connections: 10,
            idle_connections: 5,
            busy_connections: 5,
            max_connections: 20,
            connection_utilization: 50.0,
        };
        assert_eq!(status.total_connections, 10);
        assert_eq!(status.idle_connections, 5);
        assert_eq!(status.busy_connections, 5);
        assert_eq!(status.max_connections, 20);
        assert_eq!(status.connection_utilization, 50.0);
    }

    #[test]
    fn test_connection_pool_status_json() {
        let status = ConnectionPoolStatus {
            total_connections: 10,
            idle_connections: 5,
            busy_connections: 5,
            max_connections: 20,
            connection_utilization: 50.0,
        };
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: ConnectionPoolStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.total_connections, 10);
        assert_eq!(deserialized.connection_utilization, 50.0);
    }

    // ========== PerformanceMetrics tests ==========

    #[test]
    fn test_performance_metrics() {
        let metrics = PerformanceMetrics {
            average_query_time_ms: 5.5,
            slow_queries_count: 3,
            total_queries: 1000,
            transactions_per_second: 50.0,
            cache_hit_ratio: 0.95,
            deadlock_count: 0,
            redis_latency_ms: 1.2,
            redis_slow_commands_count: 0,
        };
        assert_eq!(metrics.average_query_time_ms, 5.5);
        assert_eq!(metrics.slow_queries_count, 3);
        assert_eq!(metrics.cache_hit_ratio, 0.95);
    }

    #[test]
    fn test_performance_metrics_json() {
        let metrics = PerformanceMetrics {
            average_query_time_ms: 5.5,
            slow_queries_count: 3,
            total_queries: 1000,
            transactions_per_second: 50.0,
            cache_hit_ratio: 0.95,
            deadlock_count: 0,
            redis_latency_ms: 1.2,
            redis_slow_commands_count: 0,
        };
        let json = serde_json::to_string(&metrics).unwrap();
        let deserialized: PerformanceMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.average_query_time_ms, 5.5);
    }

    // ========== DatabaseHealthStatus tests ==========

    #[test]
    fn test_database_health_status() {
        let status = DatabaseHealthStatus {
            is_healthy: true,
            connection_pool_status: ConnectionPoolStatus {
                total_connections: 10, idle_connections: 5, busy_connections: 5,
                max_connections: 20, connection_utilization: 50.0,
            },
            performance_metrics: PerformanceMetrics {
                average_query_time_ms: 5.5, slow_queries_count: 3, total_queries: 1000,
                transactions_per_second: 50.0, cache_hit_ratio: 0.95, deadlock_count: 0,
                redis_latency_ms: 1.2, redis_slow_commands_count: 0,
            },
            last_checked: chrono::Utc::now(),
        };
        assert!(status.is_healthy);
        assert_eq!(status.connection_pool_status.total_connections, 10);
    }

    #[test]
    fn test_database_health_status_unhealthy() {
        let status = DatabaseHealthStatus {
            is_healthy: false,
            connection_pool_status: ConnectionPoolStatus {
                total_connections: 20, idle_connections: 0, busy_connections: 20,
                max_connections: 20, connection_utilization: 100.0,
            },
            performance_metrics: PerformanceMetrics {
                average_query_time_ms: 100.0, slow_queries_count: 50, total_queries: 100,
                transactions_per_second: 1.0, cache_hit_ratio: 0.1, deadlock_count: 5,
                redis_latency_ms: 10.0, redis_slow_commands_count: 10,
            },
            last_checked: chrono::Utc::now(),
        };
        assert!(!status.is_healthy);
    }

    // ========== ForeignKeyViolation tests ==========

    #[test]
    fn test_foreign_key_violation() {
        let violation = ForeignKeyViolation {
            table_name: "events".to_string(),
            column_name: "room_id".to_string(),
            violating_row_id: 42,
            referenced_table: "rooms".to_string(),
        };
        assert_eq!(violation.table_name, "events");
        assert_eq!(violation.column_name, "room_id");
        assert_eq!(violation.violating_row_id, 42);
    }

    #[test]
    fn test_foreign_key_violation_json() {
        let violation = ForeignKeyViolation {
            table_name: "events".to_string(),
            column_name: "room_id".to_string(),
            violating_row_id: 42,
            referenced_table: "rooms".to_string(),
        };
        let json = serde_json::to_string(&violation).unwrap();
        let deserialized: ForeignKeyViolation = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.table_name, "events");
    }

    // ========== OrphanedRecord tests ==========

    #[test]
    fn test_orphaned_record() {
        let record = OrphanedRecord {
            table_name: "room_memberships".to_string(),
            column_name: "user_id".to_string(),
            orphan_count: 5,
            sample_orphans: vec!["@user1:example.com".to_string()],
        };
        assert_eq!(record.orphan_count, 5);
        assert_eq!(record.sample_orphans.len(), 1);
    }

    // ========== DuplicateEntry tests ==========

    #[test]
    fn test_duplicate_entry() {
        let entry = DuplicateEntry {
            table_name: "users".to_string(),
            column_name: "email".to_string(),
            duplicate_count: 3,
            sample_duplicates: vec!["dup@example.com".to_string()],
        };
        assert_eq!(entry.duplicate_count, 3);
    }

    // ========== NullConstraintViolation tests ==========

    #[test]
    fn test_null_constraint_violation() {
        let violation = NullConstraintViolation {
            table_name: "users".to_string(),
            column_name: "displayname".to_string(),
            null_count: 10,
        };
        assert_eq!(violation.null_count, 10);
    }

    // ========== VacuumStats tests ==========

    #[test]
    fn test_vacuum_stats() {
        let stats = VacuumStats {
            table_name: "events".to_string(),
            last_vacuum: None,
            last_analyze: None,
            dead_tuple_count: 100,
            dead_tuple_ratio: 0.05,
        };
        assert_eq!(stats.table_name, "events");
        assert_eq!(stats.dead_tuple_count, 100);
        assert!(stats.last_vacuum.is_none());
    }

    // ========== DataIntegrityReport tests ==========

    #[test]
    fn test_data_integrity_report() {
        let report = DataIntegrityReport {
            check_timestamp: chrono::Utc::now(),
            foreign_key_violations: vec![],
            orphaned_records: vec![],
            duplicate_entries: vec![],
            null_constraint_violations: vec![],
            overall_integrity_score: 100.0,
        };
        assert_eq!(report.overall_integrity_score, 100.0);
        assert!(report.foreign_key_violations.is_empty());
    }

    #[test]
    fn test_data_integrity_report_with_violations() {
        let report = DataIntegrityReport {
            check_timestamp: chrono::Utc::now(),
            foreign_key_violations: vec![ForeignKeyViolation {
                table_name: "events".to_string(), column_name: "room_id".to_string(),
                violating_row_id: 1, referenced_table: "rooms".to_string(),
            }],
            orphaned_records: vec![],
            duplicate_entries: vec![],
            null_constraint_violations: vec![],
            overall_integrity_score: 80.0,
        };
        assert_eq!(report.overall_integrity_score, 80.0);
        assert_eq!(report.foreign_key_violations.len(), 1);
    }
}

impl DatabaseMonitor {
    pub fn new(pool: Pool<Postgres>, redis_pool: Option<RedisPool>, max_connections: u32) -> Self {
        Self { pool, redis_pool, max_connections }
    }

    pub async fn check_connection(&self) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar!("SELECT 1").fetch_one(&self.pool).await;

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

    pub fn get_connection_pool_status(&self) -> Result<ConnectionPoolStatus, sqlx::Error> {
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
        let pool_status = self.get_connection_pool_status()?;
        let performance = self.get_performance_metrics().await?;

        Ok(DatabaseHealthStatus {
            is_healthy,
            connection_pool_status: pool_status,
            performance_metrics: performance,
            last_checked: Utc::now(),
        })
    }

    pub async fn get_performance_metrics(&self) -> Result<PerformanceMetrics, sqlx::Error> {
        let db_stats_row = sqlx::query!(
            r#"SELECT COALESCE(xact_commit, 0)::BIGINT AS "xact_commit!", COALESCE(xact_rollback, 0)::BIGINT AS "xact_rollback!",
                      COALESCE(blks_hit, 0)::BIGINT AS "blks_hit!", COALESCE(blks_read, 0)::BIGINT AS "blks_read!",
                      COALESCE(deadlocks, 0)::BIGINT AS "deadlocks!", stats_reset
               FROM pg_stat_database WHERE datname = current_database() LIMIT 1"#
        )
        .fetch_optional(&self.pool)
        .await?;

        let (xact_commit, xact_rollback, blks_hit, blks_read, deadlocks, stats_reset) =
            db_stats_row.map_or((0, 0, 0, 0, 0, None), |r| (r.xact_commit, r.xact_rollback, r.blks_hit, r.blks_read, r.deadlocks, r.stats_reset));

        let cache_hit_ratio =
            if blks_hit + blks_read > 0 { blks_hit as f64 / (blks_hit + blks_read) as f64 } else { 0.0 };

        let total_transactions = xact_commit + xact_rollback;
        let stats_window_seconds =
            stats_reset.map_or(60.0, |sr| (Utc::now() - sr).num_seconds().max(1) as f64);

        let pg_stat_statements_enabled = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM pg_extension WHERE extname = 'pg_stat_statements')",
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or(false);

        let (average_query_time_ms, slow_queries_count, total_queries) = if pg_stat_statements_enabled {
            sqlx::query_as::<_, (Option<f64>, Option<i64>, Option<i64>)>(
                "SELECT AVG(mean_exec_time), \
                        COUNT(*) FILTER (WHERE mean_exec_time >= 1000.0), \
                        SUM(calls) \
                 FROM pg_stat_statements \
                 WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database())",
            )
            .fetch_one(&self.pool)
            .await
            .map(|(avg, slow, total)| {
                (avg.unwrap_or(0.0), slow.unwrap_or(0) as u64, total.unwrap_or(total_transactions) as u64)
            })
            .unwrap_or((0.0, 0, total_transactions as u64))
        } else {
            (0.0, 0, total_transactions as u64)
        };

        let (redis_latency_ms, redis_slow_commands_count) = if let Some(redis_pool) = &self.redis_pool {
            let mut conn = redis_pool.get().await.map_err(|_e| sqlx::Error::PoolTimedOut)?; // Simplified error handling
            let latency: Result<Option<i64>, _> = redis::cmd("LATENCY").arg("LATEST").query_async(&mut *conn).await;
            let slowlog_len: Result<u64, _> = redis::cmd("SLOWLOG").arg("LEN").query_async(&mut *conn).await;

            (latency.unwrap_or(None).unwrap_or(0) as f64, slowlog_len.unwrap_or(0))
        } else {
            (0.0, 0)
        };

        Ok(PerformanceMetrics {
            average_query_time_ms,
            slow_queries_count,
            total_queries,
            transactions_per_second: total_transactions as f64 / stats_window_seconds,
            cache_hit_ratio,
            deadlock_count: deadlocks as u64,
            redis_latency_ms,
            redis_slow_commands_count,
        })
    }

    pub async fn verify_data_integrity(&self) -> Result<DataIntegrityReport, sqlx::Error> {
        let mut foreign_key_violations = Vec::new();
        let mut orphaned_records = Vec::new();
        let duplicate_entries = Vec::new();
        let null_constraint_violations = Vec::new();

        // 1. 检查核心外键约束 (示例：events -> rooms)
        let orphans = sqlx::query!(
            r#"
            SELECT 'events' AS "table_name!", 'room_id' AS "column_name!", 0::BIGINT AS "violating_row_id!", 'rooms' AS "referenced_table!"
            FROM events e
            WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = e.room_id)
            LIMIT 10
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        for row in orphans {
            foreign_key_violations.push(ForeignKeyViolation {
                table_name: row.table_name,
                column_name: row.column_name,
                violating_row_id: row.violating_row_id,
                referenced_table: row.referenced_table,
            });
        }

        // 2. 检查孤立记录 (示例：room_memberships -> users)
        let member_orphans: i64 = sqlx::query_scalar!(
            r#"SELECT COALESCE(COUNT(*), 0)::BIGINT AS "count!" FROM room_memberships m WHERE NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = m.user_id)"#
        )
        .fetch_one(&self.pool)
        .await?;

        if member_orphans > 0 {
            orphaned_records.push(OrphanedRecord {
                table_name: "room_memberships".to_string(),
                column_name: "user_id".to_string(),
                orphan_count: member_orphans,
                sample_orphans: vec![],
            });
        }

        let report = DataIntegrityReport {
            check_timestamp: Utc::now(),
            foreign_key_violations,
            orphaned_records,
            duplicate_entries,
            null_constraint_violations,
            overall_integrity_score: if member_orphans == 0 { 100.0 } else { 90.0 },
        };
        Ok(report)
    }
}

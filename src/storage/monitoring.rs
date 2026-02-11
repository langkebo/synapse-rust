use chrono::{NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::time::Duration;
use tracing::{error, info};

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
    pub average_connection_wait_time_ms: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PerformanceMetrics {
    pub average_query_time_ms: f64,
    pub slow_queries_count: u64,
    pub total_queries: u64,
    pub transactions_per_second: f64,
    pub cache_hit_ratio: f64,
    pub deadlock_count: u64,
    pub vacuum_analyze_stats: Vec<VacuumStats>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VacuumStats {
    pub table_name: String,
    pub last_vacuum: Option<NaiveDateTime>,
    pub last_analyze: Option<NaiveDateTime>,
    pub dead_tuple_count: i64,
    pub dead_tuple_ratio: f64,
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

pub struct DatabaseMonitor {
    pool: Pool<Postgres>,
    query_times: Vec<Duration>,
    slow_query_threshold: Duration,
    max_stored_queries: usize,
    max_connections: u32,
}

impl DatabaseMonitor {
    pub fn new(pool: Pool<Postgres>, max_connections: u32) -> Self {
        Self {
            pool,
            query_times: Vec::with_capacity(1000),
            slow_query_threshold: Duration::from_millis(100),
            max_stored_queries: 1000,
            max_connections,
        }
    }

    pub async fn check_connection(&self) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("SELECT 1 as check_result")
            .fetch_one(&self.pool)
            .await;

        match result {
            Ok(_) => {
                info!("数据库连接检查成功");
                Ok(true)
            }
            Err(e) => {
                error!("数据库连接检查失败: {}", e);
                Err(e)
            }
        }
    }

    pub async fn get_connection_pool_status(&self) -> Result<ConnectionPoolStatus, sqlx::Error> {
        let pool_size = self.pool.size();
        let idle_connections = self.pool.num_idle() as u32;
        let max_connections = self.max_connections;

        let status = sqlx::query_as::<_, (i64, i64)>(
            r#"
            SELECT 
                count(*) as total_connections,
                count(*) FILTER (WHERE state = 'idle') as idle_connections
            FROM pg_stat_activity 
            WHERE datname = current_database()
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(ConnectionPoolStatus {
            total_connections: status.0 as u32,
            idle_connections: status.1 as u32,
            busy_connections: pool_size.saturating_sub(idle_connections),
            max_connections,
            connection_utilization: if max_connections > 0 {
                (pool_size as f64 / max_connections as f64) * 100.0
            } else {
                0.0
            },
            average_connection_wait_time_ms: 0.0,
        })
    }

    pub async fn get_performance_metrics(&self) -> Result<PerformanceMetrics, sqlx::Error> {
        let query_times_copy: Vec<Duration> = self
            .query_times
            .iter()
            .filter(|&&t| t > Duration::ZERO)
            .cloned()
            .collect();

        let avg_query_time = if !query_times_copy.is_empty() {
            query_times_copy.iter().sum::<Duration>().as_millis() as f64
                / query_times_copy.len() as f64
        } else {
            0.0
        };

        let slow_queries = query_times_copy
            .iter()
            .filter(|&&t| t > self.slow_query_threshold)
            .count() as u64;

        let stats = sqlx::query_as::<_, (i64,)>(
            r#"
            SELECT 
                count(*)
            FROM pg_stat_activity 
            WHERE state = 'active' AND datname = current_database()
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let tps = sqlx::query_as::<_, (i64, i64)>(
            r#"
            SELECT 
                COALESCE(xact_commit, 0),
                COALESCE(xact_rollback, 0)
            FROM pg_stat_database 
            WHERE datname = current_database()
            LIMIT 1
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let vacuum_stats = self.get_vacuum_stats().await?;

        Ok(PerformanceMetrics {
            average_query_time_ms: avg_query_time,
            slow_queries_count: slow_queries,
            total_queries: stats.0 as u64,
            transactions_per_second: tps.0 as f64 / 60.0,
            cache_hit_ratio: 0.0,
            deadlock_count: 0,
            vacuum_analyze_stats: vacuum_stats,
        })
    }

    async fn get_vacuum_stats(&self) -> Result<Vec<VacuumStats>, sqlx::Error> {
        let tables = vec![
            "users",
            "devices",
            "access_tokens",
            "refresh_tokens",
            "rooms",
            "room_events",
            "room_memberships",
            "events",
        ];

        let mut vacuum_stats = Vec::new();

        for table in tables {
            let stats = sqlx::query_as::<
                _,
                (
                    i64,
                    i64,
                    Option<chrono::DateTime<chrono::Utc>>,
                    Option<chrono::DateTime<chrono::Utc>>,
                ),
            >(
                r#"
                SELECT 
                    COALESCE(n_live_tup, 0) as live_tuples,
                    COALESCE(n_dead_tup, 0) as dead_tuples,
                    last_vacuum,
                    last_analyze
                FROM pg_stat_user_tables 
                WHERE relname = $1
                "#,
            )
            .bind(table)
            .fetch_optional(&self.pool)
            .await?;

            if let Some(s) = stats {
                let total_tuples = s.0 as f64;
                let dead_tuples = s.1 as f64;
                let dead_ratio = if total_tuples > 0.0 {
                    (dead_tuples / total_tuples) * 100.0
                } else {
                    0.0
                };

                vacuum_stats.push(VacuumStats {
                    table_name: table.to_string(),
                    last_vacuum: s.2.map(|t| t.naive_utc()),
                    last_analyze: s.3.map(|t| t.naive_utc()),
                    dead_tuple_count: dead_tuples as i64,
                    dead_tuple_ratio: dead_ratio,
                });
            }
        }

        Ok(vacuum_stats)
    }

    pub async fn verify_data_integrity(&self) -> Result<DataIntegrityReport, sqlx::Error> {
        let mut report = DataIntegrityReport {
            check_timestamp: Utc::now(),
            foreign_key_violations: Vec::new(),
            orphaned_records: Vec::new(),
            duplicate_entries: Vec::new(),
            null_constraint_violations: Vec::new(),
            overall_integrity_score: 100.0,
        };

        report.foreign_key_violations = self.check_foreign_key_violations().await?;
        report.orphaned_records = self.check_orphaned_records().await?;
        report.duplicate_entries = self.check_duplicate_entries().await?;
        report.null_constraint_violations = self.check_null_constraints().await?;

        let total_issues = report.foreign_key_violations.len()
            + report.orphaned_records.len()
            + report.duplicate_entries.len()
            + report.null_constraint_violations.len();

        report.overall_integrity_score = if total_issues == 0 {
            100.0
        } else {
            (100.0 - (total_issues as f64 * 0.5)).max(0.0)
        };

        Ok(report)
    }

    async fn check_foreign_key_violations(&self) -> Result<Vec<ForeignKeyViolation>, sqlx::Error> {
        let mut violations = Vec::new();

        let fk_checks = vec![
            ("devices", "user_id", "users"),
            ("access_tokens", "user_id", "users"),
            ("access_tokens", "device_id", "devices"),
            ("refresh_tokens", "user_id", "users"),
            ("refresh_tokens", "device_id", "devices"),
            ("room_memberships", "room_id", "rooms"),
            ("room_memberships", "user_id", "users"),
            ("events", "room_id", "rooms"),
            ("events", "user_id", "users"),
        ];

        for (table, column, referenced_table) in fk_checks {
            let count = match table {
                "devices" => {
                    sqlx::query_as::<_, (i64,)>(
                        "SELECT COUNT(*) FROM devices WHERE user_id IS NOT NULL AND user_id NOT IN (SELECT user_id FROM users)",
                    )
                    .fetch_one(&self.pool)
                    .await?
                    .0
                },
                "access_tokens" => {
                    sqlx::query_as::<_, (i64,)>(
                        "SELECT COUNT(*) FROM access_tokens WHERE user_id IS NOT NULL AND user_id NOT IN (SELECT user_id FROM users)",
                    )
                    .fetch_one(&self.pool)
                    .await?
                    .0
                },
                "refresh_tokens" => {
                    sqlx::query_as::<_, (i64,)>(
                        "SELECT COUNT(*) FROM refresh_tokens WHERE user_id IS NOT NULL AND user_id NOT IN (SELECT user_id FROM users)",
                    )
                    .fetch_one(&self.pool)
                    .await?
                    .0
                },
                "room_memberships" => {
                    if column == "room_id" {
                        sqlx::query_as::<_, (i64,)>(
                            "SELECT COUNT(*) FROM room_memberships WHERE room_id IS NOT NULL AND room_id NOT IN (SELECT room_id FROM rooms)",
                        )
                        .fetch_one(&self.pool)
                        .await?
                        .0
                    } else {
                        sqlx::query_as::<_, (i64,)>(
                            "SELECT COUNT(*) FROM room_memberships WHERE user_id IS NOT NULL AND user_id NOT IN (SELECT user_id FROM users)",
                        )
                        .fetch_one(&self.pool)
                        .await?
                        .0
                    }
                },
                "events" => {
                    if column == "room_id" {
                        sqlx::query_as::<_, (i64,)>(
                            "SELECT COUNT(*) FROM events WHERE room_id IS NOT NULL AND room_id NOT IN (SELECT room_id FROM rooms)",
                        )
                        .fetch_one(&self.pool)
                        .await?
                        .0
                    } else {
                        sqlx::query_as::<_, (i64,)>(
                            "SELECT COUNT(*) FROM events WHERE user_id IS NOT NULL AND user_id NOT IN (SELECT user_id FROM users)",
                        )
                        .fetch_one(&self.pool)
                        .await?
                        .0
                    }
                },
                _ => 0,
            };

            if count > 0 {
                violations.push(ForeignKeyViolation {
                    table_name: table.to_string(),
                    column_name: column.to_string(),
                    violating_row_id: 0,
                    referenced_table: referenced_table.to_string(),
                });
            }
        }

        Ok(violations)
    }

    async fn check_orphaned_records(&self) -> Result<Vec<OrphanedRecord>, sqlx::Error> {
        let mut orphaned = Vec::new();

        let orphan_checks = vec![
            ("devices", "user_id", "users"),
            ("access_tokens", "user_id", "users"),
            ("room_memberships", "room_id", "rooms"),
            ("room_memberships", "user_id", "users"),
            ("events", "room_id", "rooms"),
        ];

        for (table, column, _referenced_table) in orphan_checks {
            let count = match table {
                "devices" => {
                    sqlx::query_as::<_, (i64,)>(
                        "SELECT COUNT(*) FROM devices WHERE user_id IS NOT NULL AND user_id NOT IN (SELECT user_id FROM users)",
                    )
                    .fetch_one(&self.pool)
                    .await?
                    .0
                },
                "access_tokens" => {
                    sqlx::query_as::<_, (i64,)>(
                        "SELECT COUNT(*) FROM access_tokens WHERE user_id IS NOT NULL AND user_id NOT IN (SELECT user_id FROM users)",
                    )
                    .fetch_one(&self.pool)
                    .await?
                    .0
                },
                "room_memberships" => {
                    if column == "room_id" {
                        sqlx::query_as::<_, (i64,)>(
                            "SELECT COUNT(*) FROM room_memberships WHERE room_id IS NOT NULL AND room_id NOT IN (SELECT room_id FROM rooms)",
                        )
                        .fetch_one(&self.pool)
                        .await?
                        .0
                    } else {
                        sqlx::query_as::<_, (i64,)>(
                            "SELECT COUNT(*) FROM room_memberships WHERE user_id IS NOT NULL AND user_id NOT IN (SELECT user_id FROM users)",
                        )
                        .fetch_one(&self.pool)
                        .await?
                        .0
                    }
                },
                "events" => {
                    sqlx::query_as::<_, (i64,)>(
                        "SELECT COUNT(*) FROM events WHERE room_id IS NOT NULL AND room_id NOT IN (SELECT room_id FROM rooms)",
                    )
                    .fetch_one(&self.pool)
                    .await?
                    .0
                },
                _ => 0,
            };

            if count > 0 {
                orphaned.push(OrphanedRecord {
                    table_name: table.to_string(),
                    column_name: column.to_string(),
                    orphan_count: count,
                    sample_orphans: Vec::new(),
                });
            }
        }

        Ok(orphaned)
    }

    async fn check_duplicate_entries(&self) -> Result<Vec<DuplicateEntry>, sqlx::Error> {
        let mut duplicates = Vec::new();

        let unique_checks = vec![("users", "user_id"), ("devices", "device_id")];

        for (table, column) in unique_checks {
            let duplicate_count = self.check_duplicate_count(table, column).await?;
            if duplicate_count > 0 {
                duplicates.push(DuplicateEntry {
                    table_name: table.to_string(),
                    column_name: column.to_string(),
                    duplicate_count,
                    sample_duplicates: Vec::new(),
                });
            }
        }

        Ok(duplicates)
    }

    async fn check_duplicate_count(&self, _table: &str, _column: &str) -> Result<i64, sqlx::Error> {
        Ok(0)
    }

    async fn check_null_constraints(&self) -> Result<Vec<NullConstraintViolation>, sqlx::Error> {
        let mut violations = Vec::new();

        let not_null_checks = vec![
            ("users", "user_id"),
            ("users", "username"),
            ("devices", "device_id"),
            ("devices", "user_id"),
        ];

        for (table, column) in not_null_checks {
            let null_count = self.check_null_count(table, column).await?;
            if null_count > 0 {
                violations.push(NullConstraintViolation {
                    table_name: table.to_string(),
                    column_name: column.to_string(),
                    null_count,
                });
            }
        }

        Ok(violations)
    }

    async fn check_null_count(&self, table: &str, column: &str) -> Result<i64, sqlx::Error> {
        let query = format!(
            "SELECT COUNT(*) as null_count FROM {} WHERE {} IS NULL",
            table, column
        );
        let row: (i64,) = sqlx::query_as(&query).fetch_one(&self.pool).await?;
        Ok(row.0)
    }

    pub fn record_query_time(&mut self, duration: Duration) {
        if self.query_times.len() >= self.max_stored_queries {
            self.query_times.remove(0);
        }
        self.query_times.push(duration);
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
}

pub async fn create_performance_stats_table(pool: &Pool<Postgres>) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS synapse_performance_stats (
            id BIGSERIAL PRIMARY KEY,
            metric_type TEXT NOT NULL,
            metric_name TEXT NOT NULL,
            metric_value DOUBLE PRECISION NOT NULL,
            collected_at BIGINT NOT NULL,
            metadata JSONB
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_performance_stats_type_time 
        ON synapse_performance_stats(metric_type, collected_at)
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn record_performance_metric(
    _pool: &Pool<Postgres>,
    _metric_type: &str,
    _metric_name: &str,
    _value: f64,
    _metadata: Option<serde_json::Value>,
) -> Result<(), sqlx::Error> {
    Ok(())
}

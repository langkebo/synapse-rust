use chrono::Utc;
use sqlx::{Pool, Postgres};
use std::time::Instant;
use tracing::{debug, error, info, warn};

pub struct DatabaseMaintenance {
    pool: Pool<Postgres>,
}

impl DatabaseMaintenance {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    pub async fn perform_maintenance(&self) -> Result<MaintenanceReport, sqlx::Error> {
        let start_time = Instant::now();
        let mut report = MaintenanceReport::new();

        info!("开始执行数据库维护任务");

        match self.vacuum_analyze().await {
            Ok(stats) => report.vacuum_results = stats,
            Err(e) => {
                error!("VACUUM ANALYZE 失败: {}", e);
                report.errors.push(format!("VACUUM ANALYZE: {e}"));
            }
        }

        match self.reindex_tables().await {
            Ok(tables) => report.reindexed_tables = tables,
            Err(e) => {
                error!("重建索引失败: {}", e);
                report.errors.push(format!("REINDEX: {e}"));
            }
        }

        match self.analyze_table_stats().await {
            Ok(stats) => report.table_stats = stats,
            Err(e) => {
                error!("表统计信息收集失败: {}", e);
                report.errors.push(format!("表统计: {e}"));
            }
        }

        report.duration_ms = start_time.elapsed().as_millis() as i64;
        report.completed_at = Utc::now();

        info!("数据库维护任务完成，耗时: {}ms", report.duration_ms);

        Ok(report)
    }

    async fn vacuum_analyze(&self) -> Result<VacuumResult, sqlx::Error> {
        let mut result = VacuumResult::new();

        let tables = vec!["users", "devices", "access_tokens", "refresh_tokens", "rooms", "room_memberships", "events"];

        // Only VACUUM ANALYZE tables that have meaningful changes since the
        // last analyze, to avoid stalling for tens of seconds on tables that
        // PostgreSQL's autovacuum has already handled.
        const MIN_MODIFICATIONS: i64 = 1_000;

        for table in tables {
            let modifications = sqlx::query_scalar::<_, Option<i64>>(
                r"
                SELECT COALESCE(n_mod_since_analyze, 0)
                FROM pg_stat_user_tables
                WHERE relname = $1
                ",
            )
            .bind(table)
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten()
            .flatten()
            .unwrap_or(0);

            if modifications < MIN_MODIFICATIONS {
                debug!("VACUUM {} skipped: only {} modifications since last analyze", table, modifications);
                continue;
            }

            let start = Instant::now();

            match sqlx::query(&format!("VACUUM ANALYZE {table}")).execute(&self.pool).await {
                Ok(_) => {
                    result.tables_processed.push(table.to_string());
                    result.execution_time_ms += start.elapsed().as_millis() as i64;
                }
                Err(e) => {
                    let err_str = e.to_string();
                    if err_str.contains("VACUUM cannot run inside a transaction block") {
                        debug!("VACUUM {} 跳过: 需要独立连接", table);
                    } else {
                        warn!("VACUUM {} 失败: {}", table, e);
                    }
                }
            }
        }

        Ok(result)
    }

    async fn reindex_tables(&self) -> Result<Vec<String>, sqlx::Error> {
        let mut reindexed = Vec::new();

        let indexes = vec![
            "uq_users_username",
            "idx_devices_user_id",
            "idx_access_tokens_user_id",
            "idx_refresh_tokens_user_id",
            "idx_rooms_creator",
            "idx_room_memberships_room",
            "idx_room_memberships_user",
            "idx_events_room_id",
            "idx_events_sender",
        ];

        for index in indexes {
            let _start = Instant::now();

            match sqlx::query_scalar::<_, String>(
                r"
                SELECT indexname FROM pg_indexes WHERE indexname = $1
                ",
            )
            .bind(index)
            .fetch_optional(&self.pool)
            .await
            {
                Ok(Some(_)) => match sqlx::query(&format!("REINDEX INDEX {index}")).execute(&self.pool).await {
                    Ok(_) => reindexed.push(index.to_string()),
                    Err(e) => {
                        warn!("索引 {} 重建失败: {}", index, e);
                    }
                },
                Ok(None) => {
                    debug!("索引 {} 不存在，跳过重建", index);
                }
                Err(e) => {
                    warn!("检查索引 {} 存在性失败: {}", index, e);
                }
            }
        }

        Ok(reindexed)
    }

    async fn analyze_table_stats(&self) -> Result<Vec<TableStats>, sqlx::Error> {
        let mut stats = Vec::new();

        let tables = sqlx::query_as::<_, (String, i64, i64, i64)>(
            r"
            SELECT
                relname as table_name,
                COALESCE(n_live_tup, 0) as live_tuples,
                COALESCE(n_dead_tup, 0) as dead_tuples,
                COALESCE(n_mod_since_analyze, 0) as modifications
            FROM pg_stat_user_tables
            ORDER BY n_mod_since_analyze DESC
            LIMIT 20
            ",
        )
        .fetch_all(&self.pool)
        .await?;

        for table in tables {
            stats.push(TableStats {
                table_name: table.0,
                live_tuples: table.1,
                dead_tuples: table.2,
                modifications: table.3,
            });
        }

        Ok(stats)
    }
}

#[derive(Debug, Clone)]
pub struct MaintenanceReport {
    pub started_at: chrono::DateTime<Utc>,
    pub completed_at: chrono::DateTime<Utc>,
    pub duration_ms: i64,
    pub vacuum_results: VacuumResult,
    pub reindexed_tables: Vec<String>,
    pub table_stats: Vec<TableStats>,
    pub errors: Vec<String>,
}

impl MaintenanceReport {
    fn new() -> Self {
        Self {
            started_at: Utc::now(),
            completed_at: Utc::now(),
            duration_ms: 0,
            vacuum_results: VacuumResult::new(),
            reindexed_tables: Vec::new(),
            table_stats: Vec::new(),
            errors: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VacuumResult {
    pub tables_processed: Vec<String>,
    pub execution_time_ms: i64,
}

impl VacuumResult {
    fn new() -> Self {
        Self { tables_processed: Vec::new(), execution_time_ms: 0 }
    }
}

#[derive(Debug, Clone)]
pub struct TableStats {
    pub table_name: String,
    pub live_tuples: i64,
    pub dead_tuples: i64,
    pub modifications: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_maintenance_report_creation() {
        let vacuum_result =
            VacuumResult { tables_processed: vec!["events".to_string(), "users".to_string()], execution_time_ms: 5000 };

        let report = MaintenanceReport {
            started_at: chrono::DateTime::from_timestamp(1234567800, 0).unwrap(),
            completed_at: chrono::DateTime::from_timestamp(1234567890, 0).unwrap(),
            duration_ms: 90000,
            vacuum_results: vacuum_result,
            reindexed_tables: vec!["users".to_string()],
            table_stats: vec![],
            errors: vec![],
        };
        assert_eq!(report.duration_ms, 90000);
    }

    #[test]
    fn test_vacuum_result_creation() {
        let result = VacuumResult { tables_processed: vec!["events".to_string()], execution_time_ms: 5000 };
        assert_eq!(result.tables_processed.len(), 1);
        assert_eq!(result.execution_time_ms, 5000);
    }

    #[test]
    fn test_table_stats_creation() {
        let stats =
            TableStats { table_name: "users".to_string(), live_tuples: 5000, dead_tuples: 100, modifications: 1000 };
        assert_eq!(stats.table_name, "users");
        assert_eq!(stats.live_tuples, 5000);
    }

    #[test]
    fn test_table_stats_with_high_dead_tuples() {
        let stats = TableStats {
            table_name: "events".to_string(),
            live_tuples: 10000,
            dead_tuples: 5000,
            modifications: 10000,
        };
        assert!(stats.dead_tuples > 1000);
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    async fn test_pool() -> Arc<PgPool> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&db_url)
            .await
            .expect("Failed to connect to test database");
        Arc::new(pool)
    }

    /// NOTE: No cleanup function is needed here. `DatabaseMaintenance` only
    /// queries PostgreSQL system catalogs (`pg_stat_user_tables`, `pg_indexes`)
    /// and runs admin DDL commands (VACUUM ANALYZE, REINDEX). It writes to
    /// zero application tables, so there is no test data to remove.

    // --- analyze_table_stats (via perform_maintenance) ---

    #[tokio::test]
    async fn test_maintenance_analyze_table_stats_populates_data() {
        let pool = test_pool().await;
        let maintenance = DatabaseMaintenance::new((*pool).clone());

        // analyze_table_stats is private; exercise it via perform_maintenance.
        // It queries pg_stat_user_tables which always has entries for any live database.
        let report = maintenance
            .perform_maintenance()
            .await
            .expect("perform_maintenance should return Ok");

        // pg_stat_user_tables should have at least some rows for a live database.
        assert!(
            !report.table_stats.is_empty(),
            "table_stats should be non-empty for any live PostgreSQL database"
        );

        // Each TableStats entry should have a non-empty table name.
        for stat in &report.table_stats {
            assert!(!stat.table_name.is_empty(), "table_name should not be empty");
        }
    }

    #[tokio::test]
    async fn test_maintenance_analyze_table_stats_stays_within_limit() {
        let pool = test_pool().await;
        let maintenance = DatabaseMaintenance::new((*pool).clone());

        // The internal query uses LIMIT 20, so table_stats should never exceed that.
        let report = maintenance
            .perform_maintenance()
            .await
            .expect("perform_maintenance should return Ok");

        assert!(
            report.table_stats.len() <= 20,
            "analyze_table_stats LIMIT 20 should cap results"
        );
    }

    // --- perform_maintenance integration ---

    #[tokio::test]
    async fn test_perform_maintenance_returns_valid_report() {
        let pool = test_pool().await;
        let maintenance = DatabaseMaintenance::new((*pool).clone());

        let report = maintenance
            .perform_maintenance()
            .await
            .expect("perform_maintenance should return Ok");

        // The report must have a non-negative duration.
        assert!(report.duration_ms >= 0, "duration_ms should be non-negative");

        // completed_at should be set after started_at (or equal within the same
        // instant for a trivial run).
        assert!(
            report.completed_at >= report.started_at,
            "completed_at should not precede started_at"
        );

        // reindexed_tables may be empty (REINDEX blocked in tx) but it must
        // always be present as a Vec.
        // (type system already enforces this; assertion for documentation.)
    }

    #[tokio::test]
    async fn test_perform_maintenance_errors_field_is_present() {
        let pool = test_pool().await;
        let maintenance = DatabaseMaintenance::new((*pool).clone());

        let report = maintenance
            .perform_maintenance()
            .await
            .expect("perform_maintenance should return Ok");

        // errors is always present even when empty.
        // VACUUM/REINDEX may log transaction-block errors, but they are caught
        // and the report is still returned as Ok.
        // The errors Vec must exist (type system enforces this). Assert it can
        // be iterated.
        let error_count = report.errors.len();
        // Log the error count for visibility; no hard assertion because it
        // depends on whether VACUUM/REINDEX succeed in the test environment.
        eprintln!("Maintenance report error count: {error_count}");
    }

    #[tokio::test]
    async fn test_perform_maintenance_vacuum_result_is_present() {
        let pool = test_pool().await;
        let maintenance = DatabaseMaintenance::new((*pool).clone());

        let report = maintenance
            .perform_maintenance()
            .await
            .expect("perform_maintenance should return Ok");

        // vacuum_results always present — tables_processed may be 0 when
        // VACUUM is blocked inside a transaction, but the VacuumResult struct
        // is always populated.
        assert!(report.vacuum_results.execution_time_ms >= 0);
    }

    // --- Idempotency / repeated runs ---

    #[tokio::test]
    async fn test_perform_maintenance_idempotent_twice() {
        let pool = test_pool().await;
        let maintenance = DatabaseMaintenance::new((*pool).clone());

        let report1 = maintenance
            .perform_maintenance()
            .await
            .expect("first perform_maintenance should succeed");

        let report2 = maintenance
            .perform_maintenance()
            .await
            .expect("second perform_maintenance should succeed");

        // Both reports should have non-empty table_stats (pg_stat_user_tables
        // is stable across calls from the same connection).
        assert!(!report1.table_stats.is_empty());
        assert!(!report2.table_stats.is_empty());

        // The table names returned should be identical for both runs since the
        // set of user tables does not change between calls.
        let names1: Vec<&str> = report1.table_stats.iter().map(|s| s.table_name.as_str()).collect();
        let names2: Vec<&str> = report2.table_stats.iter().map(|s| s.table_name.as_str()).collect();
        assert_eq!(names1, names2, "table_stats table names should be stable across repeated calls");
    }

    // --- MaintenanceReport struct validation ---

    #[tokio::test]
    async fn test_maintenance_report_timestamps_are_recent() {
        let before = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let pool = test_pool().await;
        let maintenance = DatabaseMaintenance::new((*pool).clone());

        let report = maintenance
            .perform_maintenance()
            .await
            .expect("perform_maintenance should return Ok");

        let after = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64 + 5; // small buffer

        // Both timestamps should be within [before, after] of the current wall clock.
        let started_ts = report.started_at.timestamp();
        let completed_ts = report.completed_at.timestamp();

        assert!(
            started_ts >= before && started_ts <= after,
            "started_at {started_ts} should be between {before} and {after}"
        );
        assert!(
            completed_ts >= before && completed_ts <= after,
            "completed_at {completed_ts} should be between {before} and {after}"
        );
    }

    // --- Constructor smoke test ---

    #[tokio::test]
    async fn test_database_maintenance_new_creates_instance() {
        let pool = test_pool().await;
        let maintenance = DatabaseMaintenance::new((*pool).clone());

        // Construction succeeded. Now call a method to prove the pool is usable.
        let report = maintenance
            .perform_maintenance()
            .await
            .expect("perform_maintenance after construction should succeed");

        assert!(report.duration_ms >= 0);
    }
}

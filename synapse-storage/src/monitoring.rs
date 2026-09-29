use chrono::Utc;
use deadpool_redis::Pool as RedisPool;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use synapse_common::server_metrics::ServerMetrics;
use tracing::{debug, error};

/// The `DatabaseHealthStatus` struct.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DatabaseHealthStatus {
    /// The `is_healthy` field.
    pub is_healthy: bool,
    /// The `connection_pool_status` field.
    pub connection_pool_status: ConnectionPoolStatus,
    /// The `performance_metrics` field.
    pub performance_metrics: PerformanceMetrics,
    /// The `last_checked` field.
    pub last_checked: chrono::DateTime<Utc>,
}

/// The `ConnectionPoolStatus` struct.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConnectionPoolStatus {
    /// The `total_connections` field.
    pub total_connections: u32,
    /// The `idle_connections` field.
    pub idle_connections: u32,
    /// The `busy_connections` field.
    pub busy_connections: u32,
    /// The `max_connections` field.
    pub max_connections: u32,
    /// The `connection_utilization` field.
    pub connection_utilization: f64,
}

/// The `PerformanceMetrics` struct.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PerformanceMetrics {
    /// The `average_query_time_ms` field.
    pub average_query_time_ms: f64,
    /// The `slow_queries_count` field.
    pub slow_queries_count: u64,
    /// The `total_queries` field.
    pub total_queries: u64,
    /// The `transactions_per_second` field.
    pub transactions_per_second: f64,
    /// The `cache_hit_ratio` field.
    pub cache_hit_ratio: f64,
    /// The `deadlock_count` field.
    pub deadlock_count: u64,
    /// The `redis_latency_ms` field.
    pub redis_latency_ms: f64,
    /// The `redis_slow_commands_count` field.
    pub redis_slow_commands_count: u64,
}

/// The `DataIntegrityReport` struct.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DataIntegrityReport {
    /// The `check_timestamp` field.
    pub check_timestamp: chrono::DateTime<Utc>,
    /// The `foreign_key_violations` field.
    pub foreign_key_violations: Vec<ForeignKeyViolation>,
    /// The `orphaned_records` field.
    pub orphaned_records: Vec<OrphanedRecord>,
    /// The `duplicate_entries` field.
    pub duplicate_entries: Vec<DuplicateEntry>,
    /// The `null_constraint_violations` field.
    pub null_constraint_violations: Vec<NullConstraintViolation>,
    /// The `overall_integrity_score` field.
    pub overall_integrity_score: f64,
}

/// The `ForeignKeyViolation` struct.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ForeignKeyViolation {
    /// The `table_name` field.
    pub table_name: String,
    /// The `column_name` field.
    pub column_name: String,
    /// The `violating_row_id` field.
    pub violating_row_id: i64,
    /// The `referenced_table` field.
    pub referenced_table: String,
}

/// The `OrphanedRecord` struct.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrphanedRecord {
    /// The `table_name` field.
    pub table_name: String,
    /// The `column_name` field.
    pub column_name: String,
    /// The `orphan_count` field.
    pub orphan_count: i64,
    /// The `sample_orphans` field.
    pub sample_orphans: Vec<String>,
}

/// The `DuplicateEntry` struct.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DuplicateEntry {
    /// The `table_name` field.
    pub table_name: String,
    /// The `column_name` field.
    pub column_name: String,
    /// The `duplicate_count` field.
    pub duplicate_count: i64,
    /// The `sample_duplicates` field.
    pub sample_duplicates: Vec<String>,
}

/// The `NullConstraintViolation` struct.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NullConstraintViolation {
    /// The `table_name` field.
    pub table_name: String,
    /// The `column_name` field.
    pub column_name: String,
    /// The `null_count` field.
    pub null_count: i64,
}

/// The `VacuumStats` struct.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VacuumStats {
    /// The `table_name` field.
    pub table_name: String,
    /// The `last_vacuum` field.
    pub last_vacuum: Option<chrono::NaiveDateTime>,
    /// The `last_analyze` field.
    pub last_analyze: Option<chrono::NaiveDateTime>,
    /// The `dead_tuple_count` field.
    pub dead_tuple_count: i64,
    /// The `dead_tuple_ratio` field.
    pub dead_tuple_ratio: f64,
}

/// The `DatabaseMonitor` struct.
pub struct DatabaseMonitor {
    pool: Pool<Postgres>,
    redis_pool: Option<RedisPool>,
    max_connections: u32,
    /// Reference to Prometheus metrics
    server_metrics: Option<Arc<ServerMetrics>>,
}

impl DatabaseMonitor {
    /// See [`new`].
    pub fn new(pool: Pool<Postgres>, redis_pool: Option<RedisPool>, max_connections: u32) -> Self {
        Self { pool, redis_pool, max_connections, server_metrics: None }
    }

    /// Create a new DatabaseMonitor with server metrics reference.
    pub fn with_server_metrics(
        pool: Pool<Postgres>,
        redis_pool: Option<RedisPool>,
        max_connections: u32,
        server_metrics: Arc<ServerMetrics>,
    ) -> Self {
        Self { pool, redis_pool, max_connections, server_metrics: Some(server_metrics) }
    }

    /// See [`check_connection`].
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

    /// See [`get_connection_pool_status`].
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

    /// Update pool metrics on the Prometheus collector if available.
    pub fn update_pool_metrics(&self) {
        if let Some(ref metrics) = self.server_metrics {
            let status = match self.get_connection_pool_status() {
                Ok(s) => s,
                Err(_) => return,
            };
            let is_healthy = self.pool.size() > 0;
            metrics.update_pool_metrics(
                status.busy_connections as f64,
                status.idle_connections as f64,
                status.connection_utilization / 100.0,
                is_healthy,
            );
        }
    }

    /// See [`get_full_health_status`].
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

    /// See [`get_performance_metrics`].
    pub async fn get_performance_metrics(&self) -> Result<PerformanceMetrics, sqlx::Error> {
        let db_stats = sqlx::query_as::<_, (i64, i64, i64, i64, i64, Option<chrono::DateTime<Utc>>)>(
            "SELECT COALESCE(xact_commit, 0), COALESCE(xact_rollback, 0), \
                    COALESCE(blks_hit, 0), COALESCE(blks_read, 0), COALESCE(deadlocks, 0), \
                    stats_reset \
             FROM pg_stat_database WHERE datname = current_database() LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or((0, 0, 0, 0, 0, None));

        let cache_hit_ratio =
            if db_stats.2 + db_stats.3 > 0 { db_stats.2 as f64 / (db_stats.2 + db_stats.3) as f64 } else { 0.0 };

        let total_transactions = db_stats.0 + db_stats.1;
        let stats_window_seconds =
            db_stats.5.map_or(60.0, |stats_reset| (Utc::now() - stats_reset).num_seconds().max(1) as f64);

        // D-94（吞错，D-33 同型）：原先 `.fetch_one(…).await.unwrap_or(false)` 把**任何数据库
        // 错误**静默降级成"扩展未启用" —— 监控指标从此悄悄少掉一半，而调用方看到的是一份
        // "健康的"报告。改为 `?` 传播；`EXISTS(...)` 无关系来源（R4 ①）⇒ 断言 `AS "exists!"`
        // （EXISTS 恒为 TRUE/FALSE、永不为 NULL）。
        let pg_stat_statements_enabled = sqlx::query_scalar!(
            r#"SELECT EXISTS(SELECT 1 FROM pg_extension WHERE extname = 'pg_stat_statements') AS "exists!""#,
        )
        .fetch_one(&self.pool)
        .await?;

        let (average_query_time_ms, slow_queries_count, total_queries) = if pg_stat_statements_enabled {
            // D-94（吞错）：原先 `…map(…).unwrap_or((0.0, 0, total_transactions))` 把**查询错误**
            // 也降级成"没有慢查询"，与"确实没有慢查询"不可区分 ⇒ 改 `?` 传播。
            // 三个聚合列本身在空集上会返回 NULL，那一层的 `unwrap_or` 是**真默认值**，保留。
            //
            // ⚠️ **这条查询必须保持动态（R7 结构性例外，见 §7）**：`pg_stat_statements` 是
            // **可选扩展**，baseline 迁移不创建它，本机与 CI 的库都没有该关系 ⇒ 宏在
            // `cargo sqlx prepare` 阶段无法 describe 它（实测报
            // `relation "pg_stat_statements" does not exist`），整份离线缓存都建不起来。
            // 这属于"SQL 文本是编译期常量，但**关系是否存在取决于运行环境**"——
            // 与 R6 ④ 列出的两类不可宏化情形并列的第三种。
            let row = sqlx::query_as::<_, (Option<f64>, Option<i64>, Option<i64>)>(
                "SELECT AVG(mean_exec_time), \
                        COUNT(*) FILTER (WHERE mean_exec_time >= 1000.0), \
                        SUM(calls) \
                 FROM pg_stat_statements \
                 WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database())",
            )
            .fetch_one(&self.pool)
            .await?;
            (row.0.unwrap_or(0.0), row.1.unwrap_or(0) as u64, row.2.unwrap_or(total_transactions) as u64)
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
            deadlock_count: db_stats.4 as u64,
            redis_latency_ms,
            redis_slow_commands_count,
        })
    }

    /// See [`verify_data_integrity`].
    pub async fn verify_data_integrity(&self) -> Result<DataIntegrityReport, sqlx::Error> {
        let mut foreign_key_violations = Vec::new();
        let mut orphaned_records = Vec::new();
        let duplicate_entries = Vec::new();
        let null_constraint_violations = Vec::new();

        // 1. 检查核心外键约束 (示例：events -> rooms)
        let orphans = sqlx::query_as::<_, (String, String, i64, String)>(
            r"
            SELECT 'events' as table_name, 'room_id' as column_name, 0 as violating_row_id, 'rooms' as referenced_table
            FROM events e
            WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = e.room_id)
            LIMIT 10
            ",
        )
        .fetch_all(&self.pool)
        .await?;

        for (table, col, _id, ref_table) in orphans {
            foreign_key_violations.push(ForeignKeyViolation {
                table_name: table,
                column_name: col,
                violating_row_id: 0,
                referenced_table: ref_table,
            });
        }

        // 2. 检查孤立记录 (示例：room_memberships -> users)
        let member_orphans: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM room_memberships m WHERE NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = m.user_id)",
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

/// `DatabaseMonitor` 的**真 baseline** 往返覆盖（C53-0）。
///
/// 本文件此前**没有任何测试**；四个方法里 `check_connection` 还被 `get_full_health_status`
/// 内部调用。所有用例跑在 `isolated_test_pool()` 的 per-test schema 上（R9）。
///
/// ⚠️ 覆盖里刻意钉住一个**结构性事实**（D-95，已登记为未关闭项）：
/// `verify_data_integrity` 的两条检查（`events.room_id` 无对应房间、`room_memberships.user_id`
/// 无对应用户）**结构上不可能命中** —— `fk_events_room` 与 `fk_room_memberships_user`
/// 都是外键（`ON DELETE CASCADE`），孤儿行根本无法插入。下面的用例用"插入孤儿必须失败"
/// 把真守卫（外键）证出来，同时断言该方法的报告恒为"0 违规 / 100 分"。
#[cfg(test)]
mod db_tests {
    use super::*;

    async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Pool<Postgres>) {
        let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
        let pool = (*isolated.pool()).clone();
        (isolated, pool)
    }

    #[tokio::test]
    async fn database_monitor_round_trip_on_the_migration_template() {
        let (_isolated, pool) = test_pool().await;
        let monitor = DatabaseMonitor::new(pool.clone(), None, 10);

        // check_connection：`SELECT 1` 在活库上必须为 true。
        assert!(monitor.check_connection().await.unwrap());

        // 连接池状态：total 至少 1，max 回显传入值，利用率与两者一致。
        let status = monitor.get_connection_pool_status().unwrap();
        assert!(status.total_connections >= 1);
        assert_eq!(status.max_connections, 10);
        assert_eq!(status.busy_connections, status.total_connections.saturating_sub(status.idle_connections));
        assert!(status.connection_utilization >= 0.0);

        // get_performance_metrics：`pg_stat_database` 一定有当前库的一行 ⇒ 取到真值；
        // 没有 redis pool ⇒ 两个 redis 指标为 0；命中率必须落在 [0, 1]；TPS 有限。
        let perf = monitor.get_performance_metrics().await.unwrap();
        assert_eq!(perf.redis_latency_ms, 0.0);
        assert_eq!(perf.redis_slow_commands_count, 0);
        assert!((0.0..=1.0).contains(&perf.cache_hit_ratio), "命中率越界: {}", perf.cache_hit_ratio);
        assert!(perf.transactions_per_second.is_finite());
        assert_eq!(perf.deadlock_count, 0);

        // get_full_health_status：把上面三者串起来（它内部会再调一次 check_connection）。
        let health = monitor.get_full_health_status().await.unwrap();
        assert!(health.is_healthy);
        assert_eq!(health.connection_pool_status.max_connections, 10);
        assert!((0.0..=1.0).contains(&health.performance_metrics.cache_hit_ratio));

        // verify_data_integrity：干净 schema ⇒ 0 违规、100 分。
        let report = monitor.verify_data_integrity().await.unwrap();
        assert!(report.foreign_key_violations.is_empty());
        assert!(report.orphaned_records.is_empty());
        assert!(report.duplicate_entries.is_empty());
        assert!(report.null_constraint_violations.is_empty());
        assert_eq!(report.overall_integrity_score, 100.0);

        // **真守卫是外键**（D-95）：孤儿事件/成员关系根本插不进去 ⇒ 那两条检查永远不会命中。
        let orphan_event = sqlx::query(
            "INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts) \
             VALUES ('$monitor_orphan:test', '!no_such_room:test', '@monitor:test', 'm.room.message', '{}'::jsonb, 1)",
        )
        .execute(&pool)
        .await;
        assert!(orphan_event.is_err(), "fk_events_room 必须拒绝孤儿事件");

        let orphan_membership = sqlx::query(
            "INSERT INTO room_memberships (room_id, user_id, membership) \
             VALUES ('!r:test', '@no_such_user:test', 'join')",
        )
        .execute(&pool)
        .await;
        assert!(orphan_membership.is_err(), "fk_room_memberships_user 必须拒绝孤儿成员关系");
    }
}

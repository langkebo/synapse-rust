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

/// `DataIntegrityReport`：**D-95** 重设计后的形态。
///
/// 旧形态有四个"行级违规"向量，但其中两条（`events.room_id` / `room_memberships.user_id`
/// 的孤儿扫描）**结构上不可能命中**（那两条关系由 `ON DELETE CASCADE` 外键保证，孤儿行插不
/// 进去），另两条（重复项 / NULL 约束）**从来没有生产者** ⇒ 报告恒为"0 违规 / 100 分"，
/// 是一个不会失败的门禁（铁律 8 的反面）。
///
/// 现在报告是一组**可违反**的发现（见 [`IntegrityFinding`]）：每条 kind 都有对应的
/// "构造违规 ⇒ 报告非空"用例（`monitoring.rs::db_tests`），分数按发现条数扣分。
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DataIntegrityReport {
    /// The `check_timestamp` field.
    pub check_timestamp: chrono::DateTime<Utc>,
    /// 本次巡检的发现（干净 schema 上为空）。
    pub findings: Vec<IntegrityFinding>,
    /// 0–100；干净 schema 为 100，每条发现扣 [`INTEGRITY_PENALTY_PER_FINDING`] 分。
    pub overall_integrity_score: f64,
}

/// 每条完整性发现扣的分（`overall_integrity_score` 的下限为 0）。
pub const INTEGRITY_PENALTY_PER_FINDING: f64 = 15.0;

/// 一条完整性发现：受影响的约束 + 为什么这是问题。
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct IntegrityFinding {
    /// 发现类别。
    pub kind: IntegrityFindingKind,
    /// 受影响对象，形如 `<table>.<constraint>`。
    pub subject: String,
    /// 人读说明（含"为什么这是问题"）。
    pub detail: String,
}

/// 发现的类别（每一类都有构造用例，见本文件的 `db_tests`）。
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum IntegrityFindingKind {
    /// 核心主键缺失 ⇒ 该表可能出现重复行（这是"重复项"真正能被违反的形态）。
    MissingPrimaryKey,
    /// 核心外键缺失 ⇒ 可能出现孤儿行（原实现扫的那两个场景就归在这里）。
    MissingForeignKey,
    /// 约束存在但 `NOT VALID`：PG 对**既有行**不做校验 ⇒ 现有数据可能有孤儿/越界值。
    UnvalidatedConstraint,
}

/// D-95：受周期性巡检保护的**核心约束**（名字与迁移里建的一一对应）。
///
/// 为什么是"约束在不在"而不是"扫孤儿行"：只要核心外键在且已验证，孤儿行就**插不进去**
/// （实测：`public` 上 0 条未验证约束、0 张无主键的表）—— 原实现在扫描一个恒为空集合。
/// 反过来，一旦约束被删掉或置为 `NOT VALID`，孤儿/重复行就有了存在空间 ⇒ 这才是可违反、
/// 且真能提前报警的不变量。
const REQUIRED_CONSTRAINTS: &[(&str, &str, ConstraintKind)] = &[
    // (table, constraint, kind)
    ("users", "pk_users", ConstraintKind::PrimaryKey),
    ("devices", "pk_devices", ConstraintKind::PrimaryKey),
    ("rooms", "pk_rooms", ConstraintKind::PrimaryKey),
    ("events", "pk_events", ConstraintKind::PrimaryKey),
    ("room_memberships", "pk_room_memberships", ConstraintKind::PrimaryKey),
    ("event_edges", "pk_event_edges", ConstraintKind::PrimaryKey),
    ("access_tokens", "pk_access_tokens", ConstraintKind::PrimaryKey),
    ("refresh_tokens", "pk_refresh_tokens", ConstraintKind::PrimaryKey),
    ("state_groups", "pk_state_groups", ConstraintKind::PrimaryKey),
    ("events", "fk_events_room", ConstraintKind::ForeignKey),
    ("room_memberships", "fk_room_memberships_user", ConstraintKind::ForeignKey),
    ("devices", "fk_devices_user", ConstraintKind::ForeignKey),
    ("event_edges", "fk_event_edges_prev", ConstraintKind::ForeignKey),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConstraintKind {
    PrimaryKey,
    ForeignKey,
}

impl ConstraintKind {
    fn pg_contype(self) -> &'static str {
        match self {
            ConstraintKind::PrimaryKey => "p",
            ConstraintKind::ForeignKey => "f",
        }
    }

    fn missing_finding_kind(self) -> IntegrityFindingKind {
        match self {
            ConstraintKind::PrimaryKey => IntegrityFindingKind::MissingPrimaryKey,
            ConstraintKind::ForeignKey => IntegrityFindingKind::MissingForeignKey,
        }
    }

    fn missing_detail(self, table: &str) -> String {
        match self {
            ConstraintKind::PrimaryKey => {
                format!("主键 {table} 上缺失 ⇒ 该表可能出现重复行（下一次巡检前请先恢复约束）")
            }
            ConstraintKind::ForeignKey => {
                format!("外键 {table} 上缺失 ⇒ 可能出现孤儿行（旧实现扫的那两个场景正属此类）")
            }
        }
    }
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
        // 单列 SELECT ⇒ `query_scalar!`（R6 ①：`query!` 生成的 `Map` 没有 `.execute()`）。
        // 返回值本身无意义（只为探活），错误仍然原样传播给调用方。
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
        // R6 ⑤：`query_as!` 不能构造元组 ⇒ `query!` 按字段读再组装。
        // R4 ①：五个 `COALESCE(col, 0)` 的第二实参保证结果非空（`pg_stat_database` 是系统视图，
        // Describe 不给视图列透传 NOT NULL）⇒ 逐个断言；`stats_reset` 语义上可空，不断言。
        let db_stats = sqlx::query!(
            r#"
            SELECT COALESCE(xact_commit, 0) AS "xact_commit!",
                   COALESCE(xact_rollback, 0) AS "xact_rollback!",
                   COALESCE(blks_hit, 0) AS "blks_hit!",
                   COALESCE(blks_read, 0) AS "blks_read!",
                   COALESCE(deadlocks, 0) AS "deadlocks!",
                   stats_reset
             FROM pg_stat_database WHERE datname = current_database() LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?
        .map(|r| (r.xact_commit, r.xact_rollback, r.blks_hit, r.blks_read, r.deadlocks, r.stats_reset))
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
            // ⚠️ `SUM(calls)` 必须显式 `::bigint`：PG 的 `sum(bigint)` 返回 NUMERIC，
            // 与 Rust 侧 `Option<i64>`（INT8）不兼容，运行时解码报
            // "mismatched types; Rust type `Option<i64>` (as SQL type `INT8`) is not
            // compatible with SQL type `NUMERIC`"。`calls` 本身即 bigint，转换不损失精度。
            // 本条是**动态查询，宏不做编译期校验**，故这类类型错只能在运行时暴露。
            let row = sqlx::query_as::<_, (Option<f64>, Option<i64>, Option<i64>)>(
                "SELECT AVG(mean_exec_time), \
                        COUNT(*) FILTER (WHERE mean_exec_time >= 1000.0), \
                        SUM(calls)::bigint \
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
    ///
    /// **D-95 修法**：不再扫"结构上不可能存在"的孤儿行，而是核对 [`REQUIRED_CONSTRAINTS`]
    /// 里每一条核心约束**是否存在、是否已验证**，并报告核心表上任何 `NOT VALID` 的 CHECK。
    /// 每一条发现都能被构造出来（见 `db_tests`），因此这份报告**能变红**。
    pub async fn verify_data_integrity(&self) -> Result<DataIntegrityReport, sqlx::Error> {
        let mut findings = Vec::new();

        let mut core_tables: Vec<String> = REQUIRED_CONSTRAINTS.iter().map(|(t, _, _)| (*t).to_string()).collect();
        core_tables.sort();
        core_tables.dedup();

        // 1) 核心约束的"存在性 + 已验证"：一次查询取回这些表上的全部 p/f/c 约束，
        //    再与清单逐条核对（R9：锚定 `current_schema()`，不用会回退到 `public` 的 `to_regclass`）。
        let rows = sqlx::query!(
            r#"
            SELECT t.relname AS "table_name!",
                   c.conname AS "constraint_name!",
                   c.contype::text AS "contype!",
                   c.convalidated AS "convalidated!"
            FROM pg_constraint c
            JOIN pg_class t ON t.oid = c.conrelid
            JOIN pg_namespace n ON n.oid = t.relnamespace
            WHERE n.nspname = current_schema()
              AND t.relname = ANY($1::text[])
              AND c.contype IN ('p', 'f', 'c')
            "#,
            &core_tables,
        )
        .fetch_all(&self.pool)
        .await?;

        let existing: std::collections::HashMap<(String, String), (String, bool)> =
            rows.into_iter().map(|r| ((r.table_name, r.constraint_name), (r.contype, r.convalidated))).collect();

        for (table, constraint, kind) in REQUIRED_CONSTRAINTS {
            match existing.get(&(table.to_string(), constraint.to_string())) {
                None => findings.push(IntegrityFinding {
                    kind: kind.missing_finding_kind(),
                    subject: format!("{table}.{constraint}"),
                    detail: kind.missing_detail(table),
                }),
                // 类型不符（例如主键位置被一个外键占了）同样按"缺失"处理：清单要的是这条约束。
                Some((contype, _)) if contype != kind.pg_contype() => findings.push(IntegrityFinding {
                    kind: kind.missing_finding_kind(),
                    subject: format!("{table}.{constraint}"),
                    detail: format!(
                        "{table}.{constraint} 的类型是 '{contype}'，期望 '{}' ⇒ 该约束实际上不存在",
                        kind.pg_contype()
                    ),
                }),
                Some((_, false)) => findings.push(IntegrityFinding {
                    kind: IntegrityFindingKind::UnvalidatedConstraint,
                    subject: format!("{table}.{constraint}"),
                    detail: format!(
                        "{table}.{constraint} 是 NOT VALID：PG 对既有行不校验 ⇒ 现有数据可能已经违规，\
                         请 `ALTER TABLE {table} VALIDATE CONSTRAINT {constraint}`（失败即证明有违规行）"
                    ),
                }),
                Some((_, true)) => {}
            }
        }

        // 2) 核心表上任何 `NOT VALID` 的 CHECK（清单只盯 p/f，CHECK 是"漂移"型发现）。
        for ((table, constraint), (contype, validated)) in &existing {
            if contype == "c" && !validated {
                findings.push(IntegrityFinding {
                    kind: IntegrityFindingKind::UnvalidatedConstraint,
                    subject: format!("{table}.{constraint}"),
                    detail: format!("CHECK {table}.{constraint} 是 NOT VALID ⇒ 既有行未按它校验"),
                });
            }
        }

        findings.sort_by(|a, b| a.subject.cmp(&b.subject).then(a.kind.cmp(&b.kind)));
        let overall_integrity_score = (100.0 - INTEGRITY_PENALTY_PER_FINDING * findings.len() as f64).max(0.0);

        Ok(DataIntegrityReport { check_timestamp: Utc::now(), findings, overall_integrity_score })
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

        // verify_data_integrity：干净 schema ⇒ 0 发现、100 分（D-95 重设计后的形态）。
        let report = monitor.verify_data_integrity().await.unwrap();
        assert!(report.findings.is_empty(), "干净 schema 上不应有任何发现: {:?}", report.findings);
        assert_eq!(report.overall_integrity_score, 100.0);

        // **真守卫是外键**（D-95 的原始证据）：孤儿事件/成员关系根本插不进去 ——
        // 这正是旧实现那两条扫描"永远不会命中"的原因，也是新实现改查"约束在不在/验证了吗"的理由。
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

/// D-95 的核心用例：**报告必须能变红**。
///
/// 旧实现的四个向量里，两个结构上不可能命中、另两个没有生产者 ⇒ 分数恒为 100，
/// 巡检任务里那句 `score < 80 ⇒ error!` 永远不会触发（不会失败的门禁）。
/// 这里逐条构造违规（都在 `isolated_test_pool()` 的 per-test schema 上，用完即随 schema 丢弃）：
/// ① 删掉核心主键 `pk_rooms` ⇒ `MissingPrimaryKey`；② 删掉核心外键 `fk_events_room`
/// ⇒ `MissingForeignKey`；③ 加一条 `NOT VALID` 的 CHECK ⇒ `UnvalidatedConstraint`。
/// 断言分数确实下降（而不是恒 100）。
#[cfg(test)]
mod integrity_violation_tests {
    use super::*;

    async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Pool<Postgres>) {
        let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
        let pool = (*isolated.pool()).clone();
        (isolated, pool)
    }

    #[tokio::test]
    async fn verify_data_integrity_reports_constructed_violations() {
        let (_isolated, pool) = test_pool().await;
        let monitor = DatabaseMonitor::new(pool.clone(), None, 10);

        // 前提：干净模板上没有任何发现（否则下面的断言无法归因）。
        assert!(monitor.verify_data_integrity().await.unwrap().findings.is_empty());

        // ① 主键缺失 ⇒ 该表可能出现重复行。
        // 用 `event_edges`：它的主键没有入向外键（实测 inbound FK = 0）⇒ 能直接 DROP；
        // `rooms`/`users` 的主键被二十多个外键依赖（2BP01），不适合做"删了就构造出违规"的探针。
        sqlx::query("ALTER TABLE event_edges DROP CONSTRAINT pk_event_edges")
            .execute(&pool)
            .await
            .expect("drop pk_event_edges");
        // ② 外键缺失 ⇒ 可能出现孤儿行（旧实现扫的正是这条关系）
        sqlx::query("ALTER TABLE events DROP CONSTRAINT fk_events_room").execute(&pool).await.expect("drop fk");
        // ③ NOT VALID 的 CHECK ⇒ PG 对既有行不校验
        sqlx::query("ALTER TABLE devices ADD CONSTRAINT monitor_probe_check CHECK (device_id <> '') NOT VALID")
            .execute(&pool)
            .await
            .expect("add not-valid check");

        let report = monitor.verify_data_integrity().await.unwrap();

        let kinds: Vec<IntegrityFindingKind> = report.findings.iter().map(|f| f.kind).collect();
        assert!(
            kinds.contains(&IntegrityFindingKind::MissingPrimaryKey),
            "删掉 pk_rooms 必须被发现: {:?}",
            report.findings
        );
        assert!(
            kinds.contains(&IntegrityFindingKind::MissingForeignKey),
            "删掉 fk_events_room 必须被发现: {:?}",
            report.findings
        );
        assert!(
            kinds.contains(&IntegrityFindingKind::UnvalidatedConstraint),
            "NOT VALID 的 CHECK 必须被发现: {:?}",
            report.findings
        );
        assert!(
            report.findings.iter().any(|f| f.subject == "event_edges.pk_event_edges"),
            "subject 必须指向被删的约束: {:?}",
            report.findings
        );
        assert!(
            report.overall_integrity_score < 100.0,
            "有发现时分数必须下降（旧实现恒 100）: {}",
            report.overall_integrity_score
        );
        assert_eq!(
            report.overall_integrity_score,
            (100.0 - INTEGRITY_PENALTY_PER_FINDING * report.findings.len() as f64).max(0.0),
            "分数与发现条数一致"
        );

        // 反向对照：把约束放回并 VALIDATE 之后，发现必须消失（证明这条检查是在读真 catalog，
        // 而不是恒真/恒假的常量）。
        sqlx::query("ALTER TABLE devices DROP CONSTRAINT monitor_probe_check")
            .execute(&pool)
            .await
            .expect("drop check");
        sqlx::query("ALTER TABLE event_edges ADD CONSTRAINT pk_event_edges PRIMARY KEY (event_id, prev_event_id)")
            .execute(&pool)
            .await
            .expect("restore pk");
        sqlx::query("ALTER TABLE events ADD CONSTRAINT fk_events_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE")
            .execute(&pool)
            .await
            .expect("fk");
        let restored = monitor.verify_data_integrity().await.unwrap();
        assert!(restored.findings.is_empty(), "约束恢复后必须无发现: {:?}", restored.findings);
        assert_eq!(restored.overall_integrity_score, 100.0);
    }
}

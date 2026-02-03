use chrono::{Duration, Utc};
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
                report.errors.push(format!("VACUUM ANALYZE: {}", e));
            }
        }

        match self.reindex_tables().await {
            Ok(tables) => report.reindexed_tables = tables,
            Err(e) => {
                error!("重建索引失败: {}", e);
                report.errors.push(format!("REINDEX: {}", e));
            }
        }

        match self.analyze_table_stats().await {
            Ok(stats) => report.table_stats = stats,
            Err(e) => {
                error!("表统计信息收集失败: {}", e);
                report.errors.push(format!("表统计: {}", e));
            }
        }

        match self.cleanup_expired_sessions().await {
            Ok(count) => report.expired_sessions_cleaned = count,
            Err(e) => {
                error!("过期会话清理失败: {}", e);
                report.errors.push(format!("会话清理: {}", e));
            }
        }

        report.duration_ms = start_time.elapsed().as_millis() as i64;
        report.completed_at = Utc::now();

        info!("数据库维护任务完成，耗时: {}ms", report.duration_ms);

        Ok(report)
    }

    async fn vacuum_analyze(&self) -> Result<VacuumResult, sqlx::Error> {
        let mut result = VacuumResult::new();

        let tables = vec![
            "users",
            "devices",
            "access_tokens",
            "refresh_tokens",
            "rooms",
            "room_memberships",
            "events",
            "private_messages",
            "private_sessions",
        ];

        for table in tables {
            let start = Instant::now();

            sqlx::query(&format!("VACUUM ANALYZE {}", table))
                .execute(&self.pool)
                .await?;

            result.tables_processed.push(table.to_string());
            result.execution_time_ms += start.elapsed().as_millis() as i64;
        }

        Ok(result)
    }

    async fn reindex_tables(&self) -> Result<Vec<String>, sqlx::Error> {
        let mut reindexed = Vec::new();

        let indexes = vec![
            "idx_users_username",
            "idx_devices_user",
            "idx_access_tokens_user",
            "idx_refresh_tokens_user",
            "idx_rooms_creator",
            "idx_memberships_room",
            "idx_memberships_user",
            "idx_events_room",
            "idx_events_sender",
            "idx_private_messages_session",
            "idx_private_sessions_user1",
            "idx_private_sessions_user2",
        ];

        for index in indexes {
            let _start = Instant::now();

            match sqlx::query_scalar::<_, String>(
                r#"
                SELECT indexname FROM pg_indexes WHERE indexname = $1
                "#,
            )
            .bind(index)
            .fetch_optional(&self.pool)
            .await
            {
                Ok(Some(_)) => {
                    match sqlx::query(&format!("REINDEX INDEX {}", index))
                        .execute(&self.pool)
                        .await
                    {
                        Ok(_) => reindexed.push(index.to_string()),
                        Err(e) => {
                            warn!("索引 {} 重建失败: {}", index, e);
                        }
                    }
                }
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
            r#"
            SELECT 
                relname as table_name,
                COALESCE(n_live_tup, 0) as live_tuples,
                COALESCE(n_dead_tup, 0) as dead_tuples,
                COALESCE(n_mod_since_analyze, 0) as modifications
            FROM pg_stat_user_tables
            ORDER BY n_mod_since_analyze DESC
            LIMIT 20
            "#,
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

    async fn cleanup_expired_sessions(&self) -> Result<u64, sqlx::Error> {
        let thirty_days_ago = Utc::now().naive_utc() - Duration::days(30);
        let cutoff_timestamp: i64 = thirty_days_ago.and_utc().timestamp();

        let result = sqlx::query(
            r#"
            DELETE FROM private_sessions 
            WHERE last_activity_ts < $1
            "#,
        )
        .bind(cutoff_timestamp)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
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
    pub expired_sessions_cleaned: u64,
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
            expired_sessions_cleaned: 0,
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
        Self {
            tables_processed: Vec::new(),
            execution_time_ms: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TableStats {
    pub table_name: String,
    pub live_tuples: i64,
    pub dead_tuples: i64,
    pub modifications: i64,
}

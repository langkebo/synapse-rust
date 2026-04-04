//! 批量操作工具模块
//!
//! 提供高效的批量插入、更新、删除操作

use futures::future::join_all;
use sqlx::{Pool, Postgres, Row, Sqlx};
use std::sync::Arc;
use std::time::Duration;

/// 批量操作配置
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// 每批次的大小
    pub batch_size: usize,
    /// 超时时间
    pub timeout: Duration,
    /// 最大并发批次
    pub max_concurrency: usize,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            batch_size: 100,
            timeout: Duration::from_secs(30),
            max_concurrency: 4,
        }
    }
}

/// 批量操作结果
#[derive(Debug)]
pub struct BatchResult {
    pub total_processed: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub errors: Vec<BatchError>,
}

/// 批量操作错误
#[derive(Debug)]
pub struct BatchError {
    pub index: usize,
    pub message: String,
}

/// 批量插入器
pub struct BatchInserter<T> {
    pool: Pool<Postgres>,
    config: BatchConfig,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: BatchRow> BatchInserter<T> {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self {
            pool,
            config: BatchConfig::default(),
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn with_config(mut self, config: BatchConfig) -> Self {
        self.config = config;
        self
    }

    /// 批量插入数据
    pub async fn insert(&self, items: Vec<T>) -> Result<BatchResult, sqlx::Error> {
        let mut result = BatchResult {
            total_processed: items.len(),
            success_count: 0,
            failure_count: 0,
            errors: Vec::new(),
        };

        // 分批处理
        for (batch_idx, batch) in items.chunks(self.config.batch_size).enumerate() {
            let batch_result = tokio::time::timeout(
                self.config.timeout,
                self.insert_batch(batch.to_vec(), batch_idx),
            )
            .await;

            match batch_result {
                Ok(Ok(count)) => result.success_count += count,
                Ok(Err(e)) => {
                    result.failure_count += batch.len();
                    result.errors.push(BatchError {
                        index: batch_idx,
                        message: e.to_string(),
                    });
                }
                Err(_) => {
                    result.failure_count += batch.len();
                    result.errors.push(BatchError {
                        index: batch_idx,
                        message: "Batch timeout".to_string(),
                    });
                }
            }
        }

        Ok(result)
    }

    /// 插入单批次（使用事务）
    async fn insert_batch(&self, items: Vec<T>, _batch_idx: usize) -> Result<usize, sqlx::Error> {
        if items.is_empty() {
            return Ok(0);
        }

        let mut tx = self.pool.begin().await?;
        let mut count = 0;

        for item in items {
            let sql = T::insert_sql();
            item.execute(&mut *tx, &sql).await?;
            count += 1;
        }

        tx.commit().await?;
        Ok(count)
    }

    /// 并行批量插入（使用多事务）
    pub async fn insert_parallel(&self, items: Vec<T>) -> Result<BatchResult, sqlx::Error> {
        let mut result = BatchResult {
            total_processed: items.len(),
            success_count: 0,
            failure_count: 0,
            errors: Vec::new(),
        };

        let batches: Vec<_> = items.chunks(self.config.batch_size).enumerate().collect();

        // 使用信号量控制并发
        let semaphore = Arc::new(tokio::sync::Semaphore::new(self.config.max_concurrency));
        let pool = self.pool.clone();

        let futures = batches.into_iter().map(|(idx, chunk)| {
            let semaphore = semaphore.clone();
            let pool = pool.clone();
            async move {
                let _permit = semaphore
                    .acquire()
                    .await
                    .map_err(|_| sqlx::Error::Protocol("batch semaphore closed".to_string()))?;
                
                tokio::time::timeout(
                    self.config.timeout,
                    async {
                        let mut tx = pool.begin().await?;
                        let mut count = 0;
                        
                        for item in chunk {
                            let sql = T::insert_sql();
                            item.execute(&mut *tx, &sql).await?;
                            count += 1;
                        }
                        
                        tx.commit().await?;
                        Ok::<usize, sqlx::Error>(count)
                    },
                )
                .await
                .map_err(|_| sqlx::Error::RowNotFound) // timeout error
            }
        });

        let results = join_all(futures).await;

        for (idx, r) in results.into_iter().enumerate() {
            match r {
                Ok(Ok(count)) => result.success_count += count,
                Ok(Err(e)) => {
                    result.failure_count += self.config.batch_size;
                    result.errors.push(BatchError {
                        index: idx,
                        message: e.to_string(),
                    });
                }
                Err(_) => {
                    result.failure_count += self.config.batch_size;
                    result.errors.push(BatchError {
                        index: idx,
                        message: "Parallel batch timeout".to_string(),
                    });
                }
            }
        }

        Ok(result)
    }
}

/// 批量行 trait - 实现此 trait 以支持批量插入
pub trait BatchRow: Send + Sync + Clone {
    /// 返回插入 SQL（不含 VALUES 部分）
    fn insert_sql() -> String;
    
    /// 执行单行插入
    fn execute(&self, tx: &mut sqlx::PgConnection, sql: &str) -> impl std::future::Future<Output = Result<(), sqlx::Error>> + Send;
}

// ============== 具体实现示例 ==============

/// 用户批量插入
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct BatchUser {
    pub user_id: String,
    pub username: String,
    pub password_hash: Option<String>,
    pub is_admin: bool,
    pub created_ts: i64,
}

impl BatchRow for BatchUser {
    fn insert_sql() -> String {
        r#"INSERT INTO users (user_id, username, password_hash, is_admin, created_ts)
           VALUES ($1, $2, $3, $4, $5)
           ON CONFLICT (user_id) DO NOTHING"#.to_string()
    }

    async fn execute(&self, tx: &mut sqlx::PgConnection, _sql: &str) -> Result<(), sqlx::Error> {
        sqlx::query(_sql)
            .bind(&self.user_id)
            .bind(&self.username)
            .bind(&self.password_hash)
            .bind(self.is_admin)
            .bind(self.created_ts)
            .execute(tx)
            .await?;
        Ok(())
    }
}

/// 设备批量插入
#[derive(Clone, Debug)]
pub struct BatchDevice {
    pub device_id: String,
    pub user_id: String,
    pub display_name: Option<String>,
    pub last_seen_ts: i64,
}

impl BatchRow for BatchDevice {
    fn insert_sql() -> String {
        r#"INSERT INTO devices (device_id, user_id, display_name, last_seen_ts)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT (device_id) DO NOTHING"#.to_string()
    }

    async fn execute(&self, tx: &mut sqlx::PgConnection, sql: &str) -> Result<(), sqlx::Error> {
        sqlx::query(sql)
            .bind(&self.device_id)
            .bind(&self.user_id)
            .bind(&self.display_name)
            .bind(self.last_seen_ts)
            .execute(tx)
            .await?;
        Ok(())
    }
}

/// 房间成员批量插入
#[derive(Clone, Debug)]
pub struct BatchMembership {
    pub room_id: String,
    pub user_id: String,
    pub membership: String,
    pub joined_ts: i64,
}

impl BatchRow for BatchMembership {
    fn insert_sql() -> String {
        r#"INSERT INTO room_memberships (room_id, user_id, membership, joined_ts)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT DO NOTHING"#.to_string()
    }

    async fn execute(&self, tx: &mut sqlx::PgConnection, sql: &str) -> Result<(), sqlx::Error> {
        sqlx::query(sql)
            .bind(&self.room_id)
            .bind(&self.user_id)
            .bind(&self.membership)
            .bind(self.joined_ts)
            .execute(tx)
            .await?;
        Ok(())
    }
}

// ============== 批量更新 ==============

/// 批量更新结果
pub struct BatchUpdateResult {
    pub matched_count: i64,
    pub updated_count: i64,
}

/// 批量更新工具
pub async fn batch_update<T: BatchRow + Clone>(
    pool: &Pool<Postgres>,
    items: Vec<T>,
    sql: &str,
) -> Result<BatchUpdateResult, sqlx::Error> {
    let mut total_matched = 0i64;
    let mut total_updated = 0i64;

    let mut tx = pool.begin().await?;

    for item in items {
        let result = item.execute_update(&mut *tx, sql).await?;
        total_matched += result.matched_count;
        total_updated += result.updated_count;
    }

    tx.commit().await?;

    Ok(BatchUpdateResult {
        matched_count: total_matched,
        updated_count: total_updated,
    })
}

/// 支持批量更新的 trait
pub trait BatchUpdatable: Send + Sync {
    fn execute_update(&self, tx: &mut sqlx::PgConnection, sql: &str) -> impl std::future::Future<Output = Result<UpdateResult, sqlx::Error>> + Send;
}

/// 更新结果
#[derive(Debug)]
pub struct UpdateResult {
    pub matched_count: i64,
    pub updated_count: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_config_default() {
        let config = BatchConfig::default();
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.max_concurrency, 4);
    }
}

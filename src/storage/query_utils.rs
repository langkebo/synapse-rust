//! 查询超时和重试工具
//!
//! 提供带超时和重试机制的数据库查询辅助函数

use sqlx::{Pool, Postgres, Row};
use std::time::Duration;
use thiserror::Error;

/// 查询超时错误
#[derive(Error, Debug)]
pub enum QueryTimeoutError {
    #[error("Query timed out after {0}")]
    Timeout(Duration),
    
    #[error("Query failed after {0} retries: {1}")]
    RetriesExhausted(u32, String),
    
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}

/// 默认查询超时时间
pub const DEFAULT_QUERY_TIMEOUT: Duration = Duration::from_secs(5);

/// 默认重试次数
pub const DEFAULT_MAX_RETRIES: u32 = 3;

/// 默认重试间隔
pub const DEFAULT_RETRY_DELAY: Duration = Duration::from_millis(100);

/// 带超时的查询执行器
pub struct QueryExecutor {
    pool: Pool<Postgres>,
    default_timeout: Duration,
    max_retries: u32,
    retry_delay: Duration,
}

impl QueryExecutor {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self {
            pool,
            default_timeout: DEFAULT_QUERY_TIMEOUT,
            max_retries: DEFAULT_MAX_RETRIES,
            retry_delay: DEFAULT_RETRY_DELAY,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.default_timeout = timeout;
        self
    }

    pub fn with_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// 执行带超时的查询
    pub async fn query<T, F, R>(&self, operation: F) -> Result<R, QueryTimeoutError>
    where
        F: Fn() -> futures::future::BoxFuture<'_, Result<R, sqlx::Error>>,
        R: std::fmt::Debug,
    {
        tokio::time::timeout(self.default_timeout, operation())
            .await
            .map_err(|_| QueryTimeoutError::Timeout(self.default_timeout))?
            .map_err(QueryTimeoutError::Database)
    }

    /// 执行带重试的查询（仅对临时性错误重试）
    pub async fn query_with_retry<T, F, R>(&self, operation: F) -> Result<R, QueryTimeoutError>
    where
        F: Fn() -> futures::future::BoxFuture<'_, Result<R, sqlx::Error>> + Clone,
        R: std::fmt::Debug,
    {
        let mut attempts = 0;
        let mut last_error = String::new();

        loop {
            attempts += 1;
            
            match self.query(operation.clone()).await {
                Ok(result) => return Ok(result),
                Err(QueryTimeoutError::Timeout(_)) if attempts < self.max_retries => {
                    last_error = "Timeout".to_string();
                    tokio::time::sleep(self.retry_delay * attempts).await;
                    continue;
                }
                Err(QueryTimeoutError::Database(ref e)) if attempts < self.max_retries && is_transient_error(e) => {
                    last_error = e.to_string();
                    tokio::time::sleep(self.retry_delay * attempts).await;
                    continue;
                }
                Err(e) => {
                    if attempts >= self.max_retries {
                        return Err(QueryTimeoutError::RetriesExhausted(attempts, last_error));
                    }
                    return Err(e);
                }
            }
        }
    }

    /// 执行带超时和重试的查询
    pub async fn execute<T, F>(&self, operation: F) -> Result<T, QueryTimeoutError>
    where
        F: Fn() -> futures::future::BoxFuture<'_, Result<T, sqlx::Error>> + Clone,
        T: std::fmt::Debug,
    {
        self.query_with_retry(operation).await
    }
}

/// 检查错误是否为临时性错误（可以重试）
fn is_transient_error(error: &sqlx::Error) -> bool {
    match error {
        sqlx::Error::Database(db_error) => {
            // 临时性数据库错误
            let code = db_error.code();
            matches!(
                code.as_deref(),
                Some("40001") |  // serialization_failure
                Some("53300") |  // too_many_connections
                Some("57000") |  // operator_intervention
                Some("57P01") |  // admin_shutdown
                Some("57P02") |  // crash_shutdown
                Some("57P03") |  // cannot_connect_now
                Some("HY000")    // general lock timeout
            )
        }
        sqlx::Error::PoolClosed => true,
        sqlx::Error::WorkerCrashed => true,
        _ => false,
    }
}

/// 执行带超时的批量插入
pub async fn batch_insert_with_timeout<T>(
    pool: &Pool<Postgres>,
    items: Vec<T>,
    batch_size: usize,
    timeout: Duration,
) -> Result<usize, QueryTimeoutError>
where
    T: BatchInsertable,
{
    let mut total_inserted = 0;
    
    for chunk in items.chunks(batch_size) {
        let chunk = chunk.to_vec();
        let pool = pool.clone();
        
        let inserted = tokio::time::timeout(timeout, async {
            chunk[0].batch_insert(&pool, &chunk).await
        })
        .await
        .map_err(|_| QueryTimeoutError::Timeout(timeout))??;
        
        total_inserted += inserted;
    }
    
    Ok(total_inserted)
}

/// 批量插入 trait
pub trait BatchInsertable: Send + Sync + Clone {
    fn batch_insert(&self, pool: &Pool<Postgres>, items: &[Self]) -> Result<usize, sqlx::Error>
    where
        Self: Sized;
}

/// 查询结果分页
#[derive(Debug, Clone)]
pub struct PaginatedResult<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: usize,
    pub page_size: usize,
    pub has_more: bool,
}

impl<T> PaginatedResult<T> {
    pub fn new(items: Vec<T>, total: i64, page: usize, page_size: usize) -> Self {
        let has_more = (page * page_size) < total as usize;
        Self {
            items,
            total,
            page,
            page_size,
            has_more,
        }
    }
}

/// 带分页的查询执行
pub async fn paginated_query<T, F>(
    pool: &Pool<Postgres>,
    count_sql: &str,
    data_sql: &str,
    page: usize,
    page_size: usize,
    mut mapper: F,
) -> Result<PaginatedResult<T>, sqlx::Error>
where
    F: FnMut(sqlx::postgres::PgRow<'_>) -> Result<T, sqlx::Error>,
{
    let total: i64 = sqlx::query(count_sql)
        .fetch_one(pool)
        .await?
        .get(0);

    let offset = page * page_size;
    let sql = format!("{} LIMIT {} OFFSET {}", data_sql, page_size, offset);
    
    let items = sqlx::query(&sql)
        .map(mapper)
        .fetch_all(pool)
        .await?;

    Ok(PaginatedResult::new(items, total, page, page_size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transient_error_detection() {
        // Test that transient errors are correctly identified
        assert!(is_transient_error(&sqlx::Error::PoolClosed));
    }
}

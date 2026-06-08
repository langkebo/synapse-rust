use sqlx::{PgPool, Postgres, Transaction};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransactionError {
    #[error("Transaction error: {0}")]
    Transaction(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Transaction already committed or rolled back")]
    AlreadyCompleted,

    #[error("Transaction not started")]
    NotStarted,
}

pub type TransactionResult<T> = Result<T, TransactionError>;

pub struct TransactionManager {
    pool: Arc<PgPool>,
}

impl TransactionManager {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn begin(&self) -> TransactionResult<Transaction<'static, Postgres>> {
        self.pool.begin().await.map_err(TransactionError::Database)
    }

    pub async fn begin_read_committed(&self) -> TransactionResult<Transaction<'static, Postgres>> {
        self.begin_with_isolation_level("SET TRANSACTION ISOLATION LEVEL READ COMMITTED").await
    }

    pub async fn begin_repeatable_read(&self) -> TransactionResult<Transaction<'static, Postgres>> {
        self.begin_with_isolation_level("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ").await
    }

    pub async fn begin_serializable(&self) -> TransactionResult<Transaction<'static, Postgres>> {
        self.begin_with_isolation_level("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").await
    }

    async fn begin_with_isolation_level(
        &self,
        statement: &'static str,
    ) -> TransactionResult<Transaction<'static, Postgres>> {
        let mut tx = self.pool.begin().await.map_err(TransactionError::Database)?;
        sqlx::query(statement).execute(&mut *tx).await.map_err(TransactionError::Database)?;
        Ok(tx)
    }
}

pub struct ManagedTransaction<'a> {
    transaction: Option<Transaction<'a, Postgres>>,
    committed: bool,
    rolled_back: bool,
}

impl<'a> ManagedTransaction<'a> {
    pub fn new(transaction: Transaction<'a, Postgres>) -> Self {
        Self { transaction: Some(transaction), committed: false, rolled_back: false }
    }

    pub fn transaction(&mut self) -> Result<&mut Transaction<'a, Postgres>, TransactionError> {
        self.transaction.as_mut().ok_or(TransactionError::NotStarted)
    }

    pub async fn commit(&mut self) -> TransactionResult<()> {
        if self.committed || self.rolled_back {
            return Err(TransactionError::AlreadyCompleted);
        }

        if let Some(tx) = self.transaction.take() {
            tx.commit().await.map_err(TransactionError::Database)?;
            self.committed = true;
            Ok(())
        } else {
            Err(TransactionError::NotStarted)
        }
    }

    pub async fn rollback(&mut self) -> TransactionResult<()> {
        if self.committed || self.rolled_back {
            return Err(TransactionError::AlreadyCompleted);
        }

        if let Some(tx) = self.transaction.take() {
            tx.rollback().await.map_err(TransactionError::Database)?;
            self.rolled_back = true;
            Ok(())
        } else {
            Err(TransactionError::NotStarted)
        }
    }

    pub fn is_active(&self) -> bool {
        self.transaction.is_some() && !self.committed && !self.rolled_back
    }

    pub fn is_committed(&self) -> bool {
        self.committed
    }

    pub fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl<'a> Drop for ManagedTransaction<'a> {
    fn drop(&mut self) {
        if self.transaction.is_some() && !self.committed && !self.rolled_back {
            tracing::warn!(
                "Transaction was dropped without explicit commit or rollback. \
                 The database server will roll back the transaction when the connection is reclaimed by the pool."
            );

            // Drop the transaction without attempting rollback in the Drop impl.
            // Previous implementation used block_in_place + block_on which could deadlock
            // if the connection pool is exhausted or if the current task holds resources
            // the rollback also needs. tokio::spawn is not viable because Transaction<'a>
            // is not 'static. The safe approach is to let the DB server handle cleanup
            // when the connection is returned to the pool.
            self.transaction.take();
            self.rolled_back = true;
        }
    }
}

pub async fn execute_in_transaction<F, R>(pool: &Arc<PgPool>, f: F) -> TransactionResult<R>
where
    F: FnOnce(&mut Transaction<'static, Postgres>) -> futures::future::BoxFuture<'static, Result<R, sqlx::Error>>,
{
    let mut tx = pool.begin().await.map_err(TransactionError::Database)?;

    match f(&mut tx).await {
        Ok(result) => {
            tx.commit().await.map_err(TransactionError::Database)?;
            Ok(result)
        }
        Err(e) => {
            tx.rollback().await.map_err(TransactionError::Database)?;
            Err(TransactionError::Database(e))
        }
    }
}

pub async fn execute_in_transaction_with_retry<F, R>(
    pool: &Arc<PgPool>,
    mut f: F,
    max_retries: u32,
) -> TransactionResult<R>
where
    F: FnMut(&mut Transaction<'static, Postgres>) -> futures::future::BoxFuture<'static, Result<R, sqlx::Error>>,
{
    let mut last_error = None;

    for attempt in 0..max_retries {
        let mut tx = pool.begin().await.map_err(TransactionError::Database)?;

        let result = f(&mut tx).await;

        match result {
            Ok(value) => {
                match tx.commit().await {
                    Ok(_) => return Ok(value),
                    Err(e) => {
                        if is_retryable_db_error(&e) && attempt < max_retries - 1 {
                            last_error = Some(e);
                            // No rollback needed as commit failed
                            tokio::time::sleep(tokio::time::Duration::from_millis(100 * (attempt + 1) as u64)).await;
                            continue;
                        }
                        return Err(TransactionError::Database(e));
                    }
                }
            }
            Err(e) => {
                if is_retryable_db_error(&e) && attempt < max_retries - 1 {
                    last_error = Some(e);
                    let _ = tx.rollback().await; // Ignore rollback error on retry
                    tokio::time::sleep(tokio::time::Duration::from_millis(100 * (attempt + 1) as u64)).await;
                    continue;
                }
                let _ = tx.rollback().await;
                return Err(TransactionError::Database(e));
            }
        }
    }

    Err(last_error
        .map_or_else(|| TransactionError::Transaction("Max retries exceeded".to_string()), TransactionError::Database))
}

pub fn is_retryable_db_error(error: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db_err) = error {
        let code = db_err.code().unwrap_or_default();
        let message = db_err.message().to_lowercase();

        // 40001: serialization_failure
        // 40P01: deadlock_detected
        code == "40001"
            || code == "40P01"
            || message.contains("could not serialize access")
            || message.contains("deadlock")
            || message.contains("serialization failure")
    } else {
        false
    }
}

/// 安全的 advisory lock 封装（带自动释放）
pub struct AdvisoryLockGuard {
    pool: Arc<PgPool>,
    lock_id: i64,
    acquired: bool,
}

impl AdvisoryLockGuard {
    pub async fn try_acquire(pool: &Arc<PgPool>, lock_id: i64) -> Result<Self, sqlx::Error> {
        let row: (bool,) = sqlx::query_as("SELECT pg_try_advisory_lock($1)").bind(lock_id).fetch_one(&**pool).await?;
        Ok(Self { pool: pool.clone(), lock_id, acquired: row.0 })
    }

    pub async fn acquire(pool: &Arc<PgPool>, lock_id: i64) -> Result<Self, sqlx::Error> {
        sqlx::query("SELECT pg_advisory_lock($1)").bind(lock_id).execute(&**pool).await?;
        Ok(Self { pool: pool.clone(), lock_id, acquired: true })
    }

    pub fn is_acquired(&self) -> bool {
        self.acquired
    }
}

impl Drop for AdvisoryLockGuard {
    fn drop(&mut self) {
        if self.acquired {
            let pool = self.pool.clone();
            let lock_id = self.lock_id;
            tokio::spawn(async move {
                let _ = sqlx::query("SELECT pg_advisory_unlock($1)").bind(lock_id).execute(&*pool).await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_retryable_db_error_non_db() {
        let other_err = sqlx::Error::RowNotFound;
        assert!(!is_retryable_db_error(&other_err));
    }
}

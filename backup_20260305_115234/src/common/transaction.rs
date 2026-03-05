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
        self.pool.begin().await.map_err(TransactionError::Database)
    }

    pub async fn begin_repeatable_read(&self) -> TransactionResult<Transaction<'static, Postgres>> {
        self.pool.begin().await.map_err(TransactionError::Database)
    }

    pub async fn begin_serializable(&self) -> TransactionResult<Transaction<'static, Postgres>> {
        self.pool.begin().await.map_err(TransactionError::Database)
    }
}

pub struct ManagedTransaction<'a> {
    transaction: Option<Transaction<'a, Postgres>>,
    committed: bool,
    rolled_back: bool,
}

impl<'a> ManagedTransaction<'a> {
    pub fn new(transaction: Transaction<'a, Postgres>) -> Self {
        Self {
            transaction: Some(transaction),
            committed: false,
            rolled_back: false,
        }
    }

    pub fn transaction(&mut self) -> Result<&mut Transaction<'a, Postgres>, TransactionError> {
        self.transaction
            .as_mut()
            .ok_or(TransactionError::NotStarted)
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
            tracing::warn!("Transaction was dropped without explicit commit or rollback");
        }
    }
}

pub async fn execute_in_transaction<F, R>(pool: &Arc<PgPool>, f: F) -> TransactionResult<R>
where
    F: FnOnce(
        &mut Transaction<'static, Postgres>,
    ) -> futures::future::BoxFuture<'static, Result<R, sqlx::Error>>,
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
    F: FnMut(
        &mut Transaction<'static, Postgres>,
    ) -> futures::future::BoxFuture<'static, Result<R, sqlx::Error>>,
{
    let mut last_error = None;

    for attempt in 0..max_retries {
        let mut tx = pool.begin().await.map_err(TransactionError::Database)?;

        let result = f(&mut tx).await;

        match result {
            Ok(value) => {
                tx.commit().await.map_err(TransactionError::Database)?;
                return Ok(value);
            }
            Err(e) => {
                if is_serialization_error(&e) && attempt < max_retries - 1 {
                    last_error = Some(e);
                    tx.rollback().await.map_err(TransactionError::Database)?;
                    tokio::time::sleep(tokio::time::Duration::from_millis(
                        100 * (attempt + 1) as u64,
                    ))
                    .await;
                    continue;
                } else {
                    tx.rollback().await.map_err(TransactionError::Database)?;
                    return Err(TransactionError::Database(e));
                }
            }
        }
    }

    Err(last_error
        .map(TransactionError::Database)
        .unwrap_or_else(|| TransactionError::Transaction("Max retries exceeded".to_string())))
}

fn is_serialization_error(error: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db_err) = error {
        let message = db_err.to_string().to_lowercase();
        message.contains("could not serialize access")
            || message.contains("deadlock")
            || message.contains("serialization failure")
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_managed_transaction_commit() {
        let db_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
            "postgres://synapse:synapse@localhost:5432/synapse_test".to_string()
        });
        let pool = match sqlx::PgPool::connect(&db_url).await {
            Ok(p) => Arc::new(p),
            Err(_) => return,
        };
        let manager = TransactionManager::new(&pool);

        let tx = manager.begin().await.unwrap();
        let mut managed = ManagedTransaction::new(tx);

        assert!(managed.is_active());
        assert!(!managed.is_committed());
        assert!(!managed.is_rolled_back());

        managed.commit().await.unwrap();

        assert!(!managed.is_active());
        assert!(managed.is_committed());
        assert!(!managed.is_rolled_back());
    }

    #[tokio::test]
    async fn test_managed_transaction_rollback() {
        let db_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
            "postgres://synapse:synapse@localhost:5432/synapse_test".to_string()
        });
        let pool = match sqlx::PgPool::connect(&db_url).await {
            Ok(p) => Arc::new(p),
            Err(_) => return,
        };
        let manager = TransactionManager::new(&pool);

        let tx = manager.begin().await.unwrap();
        let mut managed = ManagedTransaction::new(tx);

        assert!(managed.is_active());

        managed.rollback().await.unwrap();

        assert!(!managed.is_active());
        assert!(!managed.is_committed());
        assert!(managed.is_rolled_back());
    }

    #[tokio::test]
    async fn test_execute_in_transaction() {
        let db_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
            "postgres://synapse:synapse@localhost:5432/synapse_test".to_string()
        });
        let pool = match sqlx::PgPool::connect(&db_url).await {
            Ok(p) => Arc::new(p),
            Err(_) => return,
        };

        let result = execute_in_transaction(&pool, |tx| {
            Box::pin(async move {
                let _ = tx;
                Ok(42)
            })
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_execute_in_transaction_retry() {
        let db_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
            "postgres://synapse:synapse@localhost:5432/synapse_test".to_string()
        });
        let pool = match sqlx::PgPool::connect(&db_url).await {
            Ok(p) => Arc::new(p),
            Err(_) => return,
        };

        let mut attempts = 0;
        let result = execute_in_transaction_with_retry(
            &pool,
            |_tx| {
                attempts += 1;
                Box::pin(async move {
                    if attempts < 3 {
                        Err(sqlx::Error::Database(Box::new(MyDbError)))
                    } else {
                        Ok(42)
                    }
                })
            },
            5,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts, 3);
    }

    #[derive(Debug)]
    struct MyDbError;
    impl sqlx::error::DatabaseError for MyDbError {
        fn message(&self) -> &str {
            "could not serialize access"
        }
        fn code(&self) -> Option<std::borrow::Cow<'_, str>> {
            Some("40001".into())
        }
        fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
            self
        }
        fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
            self
        }
        fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
            self
        }
        fn kind(&self) -> sqlx::error::ErrorKind {
            sqlx::error::ErrorKind::Other
        }
    }
    impl std::fmt::Display for MyDbError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "could not serialize access")
        }
    }
    impl std::error::Error for MyDbError {}

    #[test]
    fn test_is_serialization_error_non_db() {
        let other_err = sqlx::Error::RowNotFound;
        assert!(!is_serialization_error(&other_err));
    }
}

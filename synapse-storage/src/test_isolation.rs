//! Schema isolation for test pools.
//!
//! Provides `TestPool` wrapper that connects to a dedicated test schema,
//! isolating each test from `pg_stat_*` and data pollution from other tests.
//!
//! Usage:
//! ```ignore
//! // Replace:
//! let pool = test_pool().await;
//!
//! // With:
//! let test_pool = IsolatedTestPool::new().await;
//! let pool = test_pool.pool();
//! // Schema is auto-dropped when test_pool is dropped
//! ```

use std::sync::Arc;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;

/// Creates a pool connected to a fresh isolated schema per test.
/// Each schema is created from the v11 baseline and dropped on Drop.
pub struct IsolatedTestPool {
    pool: Arc<PgPool>,
    schema: String,
}

impl IsolatedTestPool {
    /// Create a new isolated test pool with a unique schema.
    pub async fn new() -> Result<Self, sqlx::Error> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:5432/synapse_test".to_string());

        // Admin pool to create/drop schema
        let admin_pool = PgPoolOptions::new()
            .max_connections(4)
            .acquire_timeout(Duration::from_secs(60))
            .connect(&db_url)
            .await?;

        let schema = format!("test_{}", uuid::Uuid::new_v4().as_simple());

        // Create isolated schema
        sqlx::query(&format!(r#"CREATE SCHEMA "{}""#, schema))
            .execute(&admin_pool)
            .await?;

        // Clone v11 baseline into the new schema
        let baseline_sql = include_str!("../../migrations/00000000_unified_schema_v11.sql");
        let extensions_sql = include_str!("../../migrations/00000001_extensions_v10.sql");

        let set_path = format!(r#"SET search_path TO "{}", public"#, schema);

        for stmt in baseline_sql.split(';') {
            let trimmed = stmt.trim();
            if trimmed.is_empty() || trimmed.starts_with("--") || trimmed.starts_with("COPY") {
                continue;
            }
            let full = format!("{}; {}", set_path, trimmed);
            let _ = sqlx::query(&full).execute(&admin_pool).await;
        }

        for stmt in extensions_sql.split(';') {
            let trimmed = stmt.trim();
            if trimmed.is_empty() || trimmed.starts_with("--") {
                continue;
            }
            let full = format!("{}; {}", set_path, trimmed);
            let _ = sqlx::query(&full).execute(&admin_pool).await;
        }

        // Create test pool with isolated search_path
        let pool_schema = schema.clone();
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(Duration::from_secs(30))
            .after_connect(move |conn, _| {
                let schema = pool_schema.clone();
                Box::pin(async move {
                    sqlx::query(&format!(r#"SET search_path TO "{}", public"#, schema))
                        .execute(conn)
                        .await?;
                    Ok(())
                })
            })
            .connect(&db_url)
            .await?;

        Ok(Self {
            pool: Arc::new(pool),
            schema,
        })
    }

    /// Get the underlying pool.
    pub fn pool(&self) -> Arc<PgPool> {
        self.pool.clone()
    }

    /// Get the schema name for debugging.
    pub fn schema_name(&self) -> &str {
        &self.schema
    }
}

impl Drop for IsolatedTestPool {
    fn drop(&mut self) {
        let schema = self.schema.clone();
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:5432/synapse_test".to_string());

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime");

            rt.block_on(async {
                let pool = match PgPoolOptions::new()
                    .max_connections(1)
                    .acquire_timeout(Duration::from_secs(10))
                    .connect(&db_url)
                    .await
                {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::error!("Failed to connect for schema cleanup: {}", e);
                        return;
                    }
                };

                let drop_sql = format!(r#"DROP SCHEMA "{}" CASCADE"#, schema);
                match sqlx::query(&drop_sql).execute(&pool).await {
                    Ok(_) => tracing::debug!("Dropped test schema {}", schema),
                    Err(e) => tracing::error!("Failed to drop test schema {}: {}", schema, e),
                }
            });
        });
    }
}

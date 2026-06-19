use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::health::{DatabaseHealthCheck, HealthCheck};
use synapse_common::ApiError;
use synapse_storage::schema_validator::SchemaValidator;
use tracing::{instrument, warn};

#[derive(Clone)]
pub struct AdminServerService {
    pool: Arc<PgPool>,
}

impl AdminServerService {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    #[instrument(skip(self))]
    pub async fn is_database_healthy(&self) -> bool {
        let check = DatabaseHealthCheck::new((*self.pool).clone());
        let result = check.check().await;
        let is_healthy = result.status == "healthy";

        if !is_healthy {
            warn!(message = %result.message, duration_ms = result.duration_ms, "Admin DB health check failed");
        }

        is_healthy
    }

    #[instrument(skip(self, tables))]
    pub async fn validate_required_tables(&self, tables: &[&str]) -> Result<Vec<String>, ApiError> {
        let validator = SchemaValidator::new(self.pool.clone());
        validator
            .validate_required_tables(tables)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to validate required tables", &e))
    }
}

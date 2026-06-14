use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::ApiError;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AccountDataRecord {
    pub data_type: String,
    pub content: Value,
}

#[derive(Clone)]
pub struct AccountDataStorage {
    pool: Arc<PgPool>,
}

impl AccountDataStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn get_account_data_content(&self, user_id: &str, data_type: &str) -> Result<Option<Value>, ApiError> {
        sqlx::query_scalar::<_, Value>("SELECT content FROM account_data WHERE user_id = $1 AND data_type = $2")
            .bind(user_id)
            .bind(data_type)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))
    }

    pub async fn list_account_data(&self, user_id: &str) -> Result<Vec<AccountDataRecord>, ApiError> {
        sqlx::query_as::<_, AccountDataRecord>(
            "SELECT data_type, content FROM account_data WHERE user_id = $1 ORDER BY data_type ASC",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))
    }

    pub async fn delete_account_data(&self, user_id: &str, data_type: &str) -> Result<bool, ApiError> {
        let result = sqlx::query("DELETE FROM account_data WHERE user_id = $1 AND data_type = $2")
            .bind(user_id)
            .bind(data_type)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete account data", &e))?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn upsert_account_data(&self, user_id: &str, data_type: &str, content: Value) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r"
            INSERT INTO account_data (user_id, data_type, content, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $4)
            ON CONFLICT (user_id, data_type) DO UPDATE
            SET content = EXCLUDED.content, updated_ts = EXCLUDED.updated_ts
            ",
        )
        .bind(user_id)
        .bind(data_type)
        .bind(content)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to upsert account data", &e))?;
        Ok(())
    }
}

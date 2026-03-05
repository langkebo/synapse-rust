use crate::common::error::ApiError;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Filter {
    pub id: i64,
    pub user_id: String,
    pub filter_id: String,
    pub content: serde_json::Value,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateFilterRequest {
    pub user_id: String,
    pub filter_id: String,
    pub content: serde_json::Value,
}

#[derive(Clone)]
pub struct FilterStorage {
    pool: Arc<PgPool>,
}

impl FilterStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_filter(&self, request: CreateFilterRequest) -> Result<Filter, ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        let filter = sqlx::query_as::<_, Filter>(
            r#"
            INSERT INTO filters (user_id, filter_id, content, created_ts)
            VALUES ($1, $2, $3, $4)
            RETURNING id, user_id, filter_id, content, created_ts
            "#,
        )
        .bind(&request.user_id)
        .bind(&request.filter_id)
        .bind(&request.content)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create filter: {}", e)))?;

        Ok(filter)
    }

    pub async fn get_filter(
        &self,
        user_id: &str,
        filter_id: &str,
    ) -> Result<Option<Filter>, ApiError> {
        let filter = sqlx::query_as::<_, Filter>(
            r#"
            SELECT id, user_id, filter_id, content, created_ts
            FROM filters
            WHERE user_id = $1 AND filter_id = $2
            "#,
        )
        .bind(user_id)
        .bind(filter_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get filter: {}", e)))?;

        Ok(filter)
    }

    pub async fn get_filters_by_user(&self, user_id: &str) -> Result<Vec<Filter>, ApiError> {
        let filters = sqlx::query_as::<_, Filter>(
            r#"
            SELECT id, user_id, filter_id, content, created_ts
            FROM filters
            WHERE user_id = $1
            ORDER BY created_ts DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get filters: {}", e)))?;

        Ok(filters)
    }

    pub async fn delete_filter(&self, user_id: &str, filter_id: &str) -> Result<bool, ApiError> {
        let result = sqlx::query(
            r#"
            DELETE FROM filters
            WHERE user_id = $1 AND filter_id = $2
            "#,
        )
        .bind(user_id)
        .bind(filter_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete filter: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_filters_by_user(&self, user_id: &str) -> Result<u64, ApiError> {
        let result = sqlx::query(
            r#"
            DELETE FROM filters
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete filters: {}", e)))?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_filter_request() {
        let request = CreateFilterRequest {
            user_id: "@test:example.com".to_string(),
            filter_id: "filter123".to_string(),
            content: serde_json::json!({"room": {"timeline": {"limit": 100}}}),
        };
        assert_eq!(request.user_id, "@test:example.com");
        assert_eq!(request.filter_id, "filter123");
    }

    #[test]
    fn test_filter_struct() {
        let filter = Filter {
            id: 1,
            user_id: "@test:example.com".to_string(),
            filter_id: "filter123".to_string(),
            content: serde_json::json!({"room": {"timeline": {"limit": 100}}}),
            created_ts: 1234567890000,
        };
        assert_eq!(filter.id, 1);
        assert_eq!(filter.user_id, "@test:example.com");
    }
}

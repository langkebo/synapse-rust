use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::error::ApiError;

/// The `Filter` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Filter {
    /// The `id` field.
    pub id: i64,
    /// The `user_id` field.
    pub user_id: String,
    /// The `filter_id` field.
    pub filter_id: String,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `created_ts` field.
    pub created_ts: i64,
}

/// The `CreateFilterRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateFilterRequest {
    /// The `user_id` field.
    pub user_id: String,
    /// The `filter_id` field.
    pub filter_id: String,
    /// The `content` field.
    pub content: serde_json::Value,
}

/// The `FilterStoreApi` trait.
#[async_trait]
pub trait FilterStoreApi: Send + Sync {
    /// See [`create_filter`].
    async fn create_filter(&self, request: CreateFilterRequest) -> Result<Filter, ApiError>;
    /// See [`get_filter`].
    async fn get_filter(&self, user_id: &str, filter_id: &str) -> Result<Option<Filter>, ApiError>;
    /// See [`get_filters_by_user`].
    async fn get_filters_by_user(&self, user_id: &str) -> Result<Vec<Filter>, ApiError>;
    /// See [`delete_filter`].
    async fn delete_filter(&self, user_id: &str, filter_id: &str) -> Result<bool, ApiError>;
    /// See [`delete_filters_by_user`].
    async fn delete_filters_by_user(&self, user_id: &str) -> Result<u64, ApiError>;
}

/// The `FilterStorage` struct.
#[derive(Clone)]
pub struct FilterStorage {
    pool: Arc<PgPool>,
}

impl FilterStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// See [`create_filter`].
    pub async fn create_filter(&self, request: CreateFilterRequest) -> Result<Filter, ApiError> {
        let now = current_timestamp_millis();

        let filter = sqlx::query_as::<_, Filter>(
            r"
            INSERT INTO filters (user_id, filter_id, content, created_ts)
            VALUES ($1, $2, $3, $4)
            RETURNING id, user_id, filter_id, content, created_ts
            ",
        )
        .bind(&request.user_id)
        .bind(&request.filter_id)
        .bind(&request.content)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to create filter", e))?;

        Ok(filter)
    }

    /// See [`get_filter`].
    pub async fn get_filter(&self, user_id: &str, filter_id: &str) -> Result<Option<Filter>, ApiError> {
        let filter = sqlx::query_as::<_, Filter>(
            r"
            SELECT id, user_id, filter_id, content, created_ts
            FROM filters
            WHERE user_id = $1 AND filter_id = $2
            ",
        )
        .bind(user_id)
        .bind(filter_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get filter", e))?;

        Ok(filter)
    }

    /// See [`get_filters_by_user`].
    pub async fn get_filters_by_user(&self, user_id: &str) -> Result<Vec<Filter>, ApiError> {
        let filters = sqlx::query_as::<_, Filter>(
            r"
            SELECT id, user_id, filter_id, content, created_ts
            FROM filters
            WHERE user_id = $1
            ORDER BY created_ts DESC
            ",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get filters", e))?;

        Ok(filters)
    }

    /// See [`delete_filter`].
    pub async fn delete_filter(&self, user_id: &str, filter_id: &str) -> Result<bool, ApiError> {
        let result = sqlx::query(
            r"
            DELETE FROM filters
            WHERE user_id = $1 AND filter_id = $2
            ",
        )
        .bind(user_id)
        .bind(filter_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to delete filter", e))?;

        Ok(result.rows_affected() > 0)
    }

    /// See [`delete_filters_by_user`].
    pub async fn delete_filters_by_user(&self, user_id: &str) -> Result<u64, ApiError> {
        let result = sqlx::query(
            r"
            DELETE FROM filters
            WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to delete filters", e))?;

        Ok(result.rows_affected())
    }
}

#[async_trait]
impl FilterStoreApi for FilterStorage {
    async fn create_filter(&self, request: CreateFilterRequest) -> Result<Filter, ApiError> {
        self.create_filter(request).await
    }

    async fn get_filter(&self, user_id: &str, filter_id: &str) -> Result<Option<Filter>, ApiError> {
        self.get_filter(user_id, filter_id).await
    }

    async fn get_filters_by_user(&self, user_id: &str) -> Result<Vec<Filter>, ApiError> {
        self.get_filters_by_user(user_id).await
    }

    async fn delete_filter(&self, user_id: &str, filter_id: &str) -> Result<bool, ApiError> {
        self.delete_filter(user_id, filter_id).await
    }

    async fn delete_filters_by_user(&self, user_id: &str) -> Result<u64, ApiError> {
        self.delete_filters_by_user(user_id).await
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

#[cfg(test)]
mod db_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::sync::Arc;

    async fn test_pool() -> Arc<PgPool> {
        crate::test_utils::connect_shared_test_pool()
            .await
            .expect("test database must be reachable - a swallowed error here surfaces later as an unrelated failure")
    }

    fn make_suffix() -> String {
        uuid::Uuid::new_v4().simple().to_string()
    }

    fn make_request(user_id: &str, filter_id: &str) -> CreateFilterRequest {
        CreateFilterRequest {
            user_id: user_id.to_string(),
            filter_id: filter_id.to_string(),
            content: serde_json::json!({"room": {"timeline": {"limit": 100}}}),
        }
    }

    #[tokio::test]
    async fn create_filter_then_get() {
        let pool = test_pool().await;
        let storage = FilterStorage::new(&pool);
        let suffix = make_suffix();
        let user_id = format!("@filter_create_{suffix}:test");
        let filter_id = format!("filter_{suffix}");

        let created = storage.create_filter(make_request(&user_id, &filter_id)).await.unwrap();
        assert_eq!(created.user_id, user_id);
        assert_eq!(created.filter_id, filter_id);

        let fetched = storage.get_filter(&user_id, &filter_id).await.unwrap().unwrap();
        assert_eq!(fetched.content, serde_json::json!({"room": {"timeline": {"limit": 100}}}));

        let _ = sqlx::query("DELETE FROM filters WHERE user_id = $1").bind(&user_id).execute(pool.as_ref()).await;
    }

    #[tokio::test]
    async fn get_filter_none_for_missing() {
        let pool = test_pool().await;
        let storage = FilterStorage::new(&pool);
        let suffix = make_suffix();
        let user_id = format!("@filter_missing_{suffix}:test");
        assert!(storage.get_filter(&user_id, "nonexistent").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_filters_by_user_returns_multiple() {
        let pool = test_pool().await;
        let storage = FilterStorage::new(&pool);
        let suffix = make_suffix();
        let user_id = format!("@filter_multi_{suffix}:test");

        storage.create_filter(make_request(&user_id, &format!("a_{suffix}"))).await.unwrap();
        storage.create_filter(make_request(&user_id, &format!("b_{suffix}"))).await.unwrap();

        let filters = storage.get_filters_by_user(&user_id).await.unwrap();
        assert_eq!(filters.len(), 2);

        let _ = sqlx::query("DELETE FROM filters WHERE user_id = $1").bind(&user_id).execute(pool.as_ref()).await;
    }

    #[tokio::test]
    async fn delete_filter_removes_record() {
        let pool = test_pool().await;
        let storage = FilterStorage::new(&pool);
        let suffix = make_suffix();
        let user_id = format!("@filter_delete_{suffix}:test");
        let filter_id = format!("filter_{suffix}");

        storage.create_filter(make_request(&user_id, &filter_id)).await.unwrap();
        assert!(storage.delete_filter(&user_id, &filter_id).await.unwrap());
        assert!(storage.get_filter(&user_id, &filter_id).await.unwrap().is_none());
        // 再次删除返回 false（记录已不存在）
        assert!(!storage.delete_filter(&user_id, &filter_id).await.unwrap());
    }

    #[tokio::test]
    async fn delete_filters_by_user_removes_all() {
        let pool = test_pool().await;
        let storage = FilterStorage::new(&pool);
        let suffix = make_suffix();
        let user_id = format!("@filter_delete_all_{suffix}:test");

        storage.create_filter(make_request(&user_id, &format!("a_{suffix}"))).await.unwrap();
        storage.create_filter(make_request(&user_id, &format!("b_{suffix}"))).await.unwrap();

        let removed = storage.delete_filters_by_user(&user_id).await.unwrap();
        assert_eq!(removed, 2);
        assert!(storage.get_filters_by_user(&user_id).await.unwrap().is_empty());
    }
}

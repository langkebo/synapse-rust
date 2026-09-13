use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::ApiError;

/// The `AccountDataRecord` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AccountDataRecord {
    /// The `data_type` field.
    pub data_type: String,
    /// The `content` field.
    pub content: Value,
}

/// The `AccountDataStoreApi` trait.
#[async_trait::async_trait]
pub trait AccountDataStoreApi: Send + Sync + std::fmt::Debug {
    /// See [`get_account_data_content`].
    async fn get_account_data_content(&self, user_id: &str, data_type: &str) -> Result<Option<Value>, ApiError>;
    /// See [`list_account_data`].
    async fn list_account_data(&self, user_id: &str) -> Result<Vec<AccountDataRecord>, ApiError>;
    /// See [`delete_account_data`].
    async fn delete_account_data(&self, user_id: &str, data_type: &str) -> Result<bool, ApiError>;
    /// See [`upsert_account_data`].
    async fn upsert_account_data(&self, user_id: &str, data_type: &str, content: Value) -> Result<(), ApiError>;
}

/// The `AccountDataStorage` struct.
#[derive(Clone, Debug)]
pub struct AccountDataStorage {
    pool: Arc<PgPool>,
}

impl AccountDataStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }
}

#[async_trait::async_trait]
impl AccountDataStoreApi for AccountDataStorage {
    async fn get_account_data_content(&self, user_id: &str, data_type: &str) -> Result<Option<Value>, ApiError> {
        sqlx::query_scalar::<_, Value>("SELECT content FROM account_data WHERE user_id = $1 AND data_type = $2")
            .bind(user_id)
            .bind(data_type)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Database error", e))
    }

    async fn list_account_data(&self, user_id: &str) -> Result<Vec<AccountDataRecord>, ApiError> {
        sqlx::query_as::<_, AccountDataRecord>(
            "SELECT data_type, content FROM account_data WHERE user_id = $1 ORDER BY data_type ASC",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Database error", e))
    }

    async fn delete_account_data(&self, user_id: &str, data_type: &str) -> Result<bool, ApiError> {
        let result = sqlx::query("DELETE FROM account_data WHERE user_id = $1 AND data_type = $2")
            .bind(user_id)
            .bind(data_type)
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to delete account data", e))?;
        Ok(result.rows_affected() > 0)
    }

    async fn upsert_account_data(&self, user_id: &str, data_type: &str, content: Value) -> Result<(), ApiError> {
        let now = current_timestamp_millis();
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
        .map_err(|e| ApiError::internal_with_cause("Failed to upsert account data", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use serde_json::json;

    async fn test_pool() -> Arc<PgPool> {
        crate::test_utils::connect_shared_test_pool()
            .await
            .expect("test database must be reachable - a swallowed error here surfaces later as an unrelated failure")
    }

    fn unique_suffix() -> String {
        uuid::Uuid::new_v4().simple().to_string()
    }

    fn test_user_id(suffix: &str) -> String {
        format!("@ad_test_{suffix}:localhost")
    }

    async fn ensure_test_user(pool: &PgPool, user_id: &str) {
        let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
        sqlx::query(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, EXTRACT(EPOCH FROM NOW()) * 1000) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(username)
        .execute(pool)
        .await
        .expect("test fixture: insert must succeed - a swallowed error here surfaces later as an unrelated failure");
    }

    async fn clean_account_data(pool: &PgPool, suffix: &str) {
        let pattern = format!("%{}%", suffix);
        sqlx::query("DELETE FROM account_data WHERE user_id LIKE $1 AND data_type LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await
            .expect(
                "test fixture: delete must succeed - a swallowed error here surfaces later as an unrelated failure",
            );
    }

    #[tokio::test]
    async fn test_upsert_and_get() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = test_user_id(&suffix);
        ensure_test_user(&pool, &user_id).await;
        clean_account_data(&pool, &suffix).await;

        let storage = AccountDataStorage::new(&pool);
        let data_type = "im.vector.test.type";
        let content = json!({"hello": "world", "number": 42});

        storage.upsert_account_data(&user_id, data_type, content.clone()).await.expect("upsert should succeed");

        let result = storage.get_account_data_content(&user_id, data_type).await.expect("get should succeed");

        assert_eq!(result, Some(content));

        clean_account_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_not_found() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = test_user_id(&suffix);
        ensure_test_user(&pool, &user_id).await;
        clean_account_data(&pool, &suffix).await;

        let storage = AccountDataStorage::new(&pool);

        let result = storage
            .get_account_data_content(&user_id, "nonexistent.type")
            .await
            .expect("get should succeed for missing data");

        assert_eq!(result, None);

        clean_account_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_list_multiple() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = test_user_id(&suffix);
        ensure_test_user(&pool, &user_id).await;
        clean_account_data(&pool, &suffix).await;

        let storage = AccountDataStorage::new(&pool);

        storage.upsert_account_data(&user_id, "type.a", json!({"val": 1})).await.expect("upsert a");
        storage.upsert_account_data(&user_id, "type.b", json!({"val": 2})).await.expect("upsert b");
        storage.upsert_account_data(&user_id, "type.c", json!({"val": 3})).await.expect("upsert c");

        let records = storage.list_account_data(&user_id).await.expect("list should succeed");

        assert_eq!(records.len(), 3, "should have 3 account data records");
        assert_eq!(records[0].data_type, "type.a");
        assert_eq!(records[0].content, json!({"val": 1}));
        assert_eq!(records[1].data_type, "type.b");
        assert_eq!(records[1].content, json!({"val": 2}));
        assert_eq!(records[2].data_type, "type.c");
        assert_eq!(records[2].content, json!({"val": 3}));

        clean_account_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_list_empty() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = test_user_id(&suffix);
        ensure_test_user(&pool, &user_id).await;
        clean_account_data(&pool, &suffix).await;

        let storage = AccountDataStorage::new(&pool);

        let records = storage.list_account_data(&user_id).await.expect("list should succeed for user with no data");

        assert!(records.is_empty(), "list should return empty vec");

        clean_account_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_delete_existing() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = test_user_id(&suffix);
        ensure_test_user(&pool, &user_id).await;
        clean_account_data(&pool, &suffix).await;

        let storage = AccountDataStorage::new(&pool);
        let data_type = "im.vector.test.deletable";

        storage
            .upsert_account_data(&user_id, data_type, json!({"to_delete": true}))
            .await
            .expect("upsert should succeed");

        let deleted = storage.delete_account_data(&user_id, data_type).await.expect("delete should succeed");

        assert!(deleted, "delete should return true when row exists");

        let result = storage.get_account_data_content(&user_id, data_type).await.expect("get should succeed");

        assert_eq!(result, None, "data should be gone after delete");

        clean_account_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_delete_not_found() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = test_user_id(&suffix);
        ensure_test_user(&pool, &user_id).await;
        clean_account_data(&pool, &suffix).await;

        let storage = AccountDataStorage::new(&pool);

        let deleted =
            storage.delete_account_data(&user_id, "never.inserted.type").await.expect("delete should succeed");

        assert!(!deleted, "delete should return false when no row exists");

        clean_account_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_overwrite_account_data() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = test_user_id(&suffix);
        ensure_test_user(&pool, &user_id).await;
        clean_account_data(&pool, &suffix).await;

        let storage = AccountDataStorage::new(&pool);
        let data_type = "im.vector.test.overwrite";

        storage.upsert_account_data(&user_id, data_type, json!({"version": 1})).await.expect("first upsert");

        storage
            .upsert_account_data(&user_id, data_type, json!({"version": 2, "updated": true}))
            .await
            .expect("second upsert (overwrite)");

        let result = storage
            .get_account_data_content(&user_id, data_type)
            .await
            .expect("get should succeed")
            .expect("content should exist after overwrite");

        assert_eq!(result, json!({"version": 2, "updated": true}));

        clean_account_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_multiple_users_isolation() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_a = format!("@ad_test_a_{suffix}:localhost");
        let user_b = format!("@ad_test_b_{suffix}:localhost");
        ensure_test_user(&pool, &user_a).await;
        ensure_test_user(&pool, &user_b).await;
        clean_account_data(&pool, &suffix).await;

        let storage = AccountDataStorage::new(&pool);
        let data_type = "im.vector.test.shared_type";

        storage.upsert_account_data(&user_a, data_type, json!({"owner": "a"})).await.expect("upsert for user_a");
        storage.upsert_account_data(&user_b, data_type, json!({"owner": "b"})).await.expect("upsert for user_b");

        let a_content = storage
            .get_account_data_content(&user_a, data_type)
            .await
            .expect("get for user_a")
            .expect("user_a should have data");
        let b_content = storage
            .get_account_data_content(&user_b, data_type)
            .await
            .expect("get for user_b")
            .expect("user_b should have data");

        assert_eq!(a_content, json!({"owner": "a"}));
        assert_eq!(b_content, json!({"owner": "b"}));

        let a_records = storage.list_account_data(&user_a).await.expect("list for user_a");
        let b_records = storage.list_account_data(&user_b).await.expect("list for user_b");

        assert_eq!(a_records.len(), 1);
        assert_eq!(b_records.len(), 1);

        clean_account_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_nested_json_preservation() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = test_user_id(&suffix);
        ensure_test_user(&pool, &user_id).await;
        clean_account_data(&pool, &suffix).await;

        let storage = AccountDataStorage::new(&pool);
        let data_type = "im.vector.test.nested";
        let complex = json!({
            "string": "hello",
            "number": 3.5,
            "bool": true,
            "null_val": null,
            "array": [1, 2, {"deep": "value"}],
            "object": {
                "nested": {
                    "key": "data"
                }
            }
        });

        storage.upsert_account_data(&user_id, data_type, complex.clone()).await.expect("upsert complex json");

        let result = storage
            .get_account_data_content(&user_id, data_type)
            .await
            .expect("get should succeed")
            .expect("content should exist");

        assert_eq!(result, complex);

        clean_account_data(&pool, &suffix).await;
    }
}

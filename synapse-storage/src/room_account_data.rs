use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use synapse_common::ApiError;

/// The `RoomAccountDataStoreApi` trait.
#[async_trait]
pub trait RoomAccountDataStoreApi: Send + Sync {
    /// See [`get_room_account_data_content`].
    async fn get_room_account_data_content(
        &self,
        user_id: &str,
        room_id: &str,
        data_type: &str,
    ) -> Result<Option<serde_json::Value>, ApiError>;
    /// See [`get_room_account_data_with_ts`].
    async fn get_room_account_data_with_ts(
        &self,
        user_id: &str,
        room_id: &str,
        data_type: &str,
    ) -> Result<Option<(serde_json::Value, Option<i64>)>, ApiError>;
    /// See [`list_room_account_data`].
    async fn list_room_account_data(
        &self,
        user_id: &str,
        room_id: &str,
    ) -> Result<Vec<RoomAccountDataRecord>, ApiError>;
    /// See [`list_room_account_data_batch`].
    async fn list_room_account_data_batch(
        &self,
        user_id: &str,
        room_ids: &[String],
    ) -> Result<Vec<RoomAccountDataRecord>, ApiError>;
    /// See [`upsert_room_account_data`].
    async fn upsert_room_account_data(
        &self,
        user_id: &str,
        room_id: &str,
        data_type: &str,
        data: &serde_json::Value,
        now: i64,
    ) -> Result<(), sqlx::Error>;
    /// See [`delete_room_account_data`].
    async fn delete_room_account_data(&self, user_id: &str, room_id: &str, data_type: &str) -> Result<bool, ApiError>;
}

/// The `RoomAccountDataStorage` struct.
pub struct RoomAccountDataStorage {
    pool: Arc<sqlx::PgPool>,
}

/// The `RoomAccountDataRecord` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RoomAccountDataRecord {
    /// The `room_id` field.
    pub room_id: String,
    /// The `data_type` field.
    pub data_type: String,
    /// The `content` field.
    pub content: Value,
}

impl RoomAccountDataStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<sqlx::PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// See [`get_room_account_data_content`].
    pub async fn get_room_account_data_content(
        &self,
        user_id: &str,
        room_id: &str,
        data_type: &str,
    ) -> Result<Option<Value>, ApiError> {
        // D-93（先修）：原先这里转手调用 `get_room_account_data()`，而那个方法（以及 trait 里的
        // 同名条目）把 `sqlx::postgres::PgRow` 直接暴露成**公共 API** —— 存储层的行类型泄漏到
        // 服务/路由层，mock 也只能 `unimplemented!()`（它的报错信息自己写着
        // "use get_room_account_data_content"）。该 raw-row 方法全仓**无外部调用者**
        // ⇒ 删除，查询内联到这里并直接用宏（R1：新写的 SQL 一律静态化）。
        // `data` 是 `JSONB NOT NULL` ⇒ `query_scalar!` 的 `fetch_optional` 正好给 `Option<Value>`。
        let content = sqlx::query_scalar!(
            r#"SELECT data FROM room_account_data WHERE user_id = $1 AND room_id = $2 AND data_type = $3"#,
            user_id,
            room_id,
            data_type
        )
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(|e| ApiError::internal_with_cause("Database error", e))?;
        Ok(content)
    }

    /// See [`get_room_account_data_with_ts`].
    pub async fn get_room_account_data_with_ts(
        &self,
        user_id: &str,
        room_id: &str,
        data_type: &str,
    ) -> Result<Option<(Value, Option<i64>)>, ApiError> {
        // D-93 家族（**吞错**，D-33/D-72 同型）：原先
        // `row.try_get::<Option<i64>, _>("updated_ts").ok().flatten()` 把解码失败静默变成 `None`，
        // 与"该列本就是 NULL"不可区分（而 schema 里 `updated_ts` 是 `BIGINT NOT NULL`）。
        // 改成宏后列类型由真 catalog 定型 —— 可空性在编译期钉死，运行期不再有这条吞错路径。
        // 返回值仍是 `Option<i64>`（`Some(ts)`），与原先"成功解码"的行为逐字一致。
        let row = sqlx::query!(
            r#"SELECT data, updated_ts FROM room_account_data WHERE user_id = $1 AND room_id = $2 AND data_type = $3"#,
            user_id,
            room_id,
            data_type
        )
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(|e| ApiError::internal_with_cause("Database error", e))?;

        Ok(row.map(|row| (row.data, Some(row.updated_ts))))
    }

    /// See [`list_room_account_data`].
    pub async fn list_room_account_data(
        &self,
        user_id: &str,
        room_id: &str,
    ) -> Result<Vec<RoomAccountDataRecord>, ApiError> {
        sqlx::query_as::<_, RoomAccountDataRecord>(
            "SELECT room_id, data_type, data AS content \
             FROM room_account_data \
             WHERE user_id = $1 AND room_id = $2 \
             ORDER BY data_type ASC",
        )
        .bind(user_id)
        .bind(room_id)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(|e| ApiError::internal_with_cause("Database error", e))
    }

    /// See [`list_room_account_data_batch`].
    pub async fn list_room_account_data_batch(
        &self,
        user_id: &str,
        room_ids: &[String],
    ) -> Result<Vec<RoomAccountDataRecord>, ApiError> {
        if room_ids.is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query_as::<_, RoomAccountDataRecord>(
            "SELECT room_id, data_type, data AS content \
             FROM room_account_data \
             WHERE user_id = $1 AND room_id = ANY($2) \
             ORDER BY room_id ASC, data_type ASC",
        )
        .bind(user_id)
        .bind(room_ids)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(|e| ApiError::internal_with_cause("Database error", e))
    }

    /// See [`upsert_room_account_data`].
    pub async fn upsert_room_account_data(
        &self,
        user_id: &str,
        room_id: &str,
        data_type: &str,
        data: &Value,
        now: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO room_account_data (user_id, room_id, data_type, data, created_ts, updated_ts) \
             VALUES ($1, $2, $3, $4, $5, $5) \
             ON CONFLICT (user_id, room_id, data_type) \
             DO UPDATE SET data = EXCLUDED.data, updated_ts = EXCLUDED.updated_ts",
        )
        .bind(user_id)
        .bind(room_id)
        .bind(data_type)
        .bind(data)
        .bind(now)
        .execute(self.pool.as_ref())
        .await?;
        Ok(())
    }

    /// See [`delete_room_account_data`].
    pub async fn delete_room_account_data(
        &self,
        user_id: &str,
        room_id: &str,
        data_type: &str,
    ) -> Result<bool, ApiError> {
        let result =
            sqlx::query("DELETE FROM room_account_data WHERE user_id = $1 AND room_id = $2 AND data_type = $3")
                .bind(user_id)
                .bind(room_id)
                .bind(data_type)
                .execute(self.pool.as_ref())
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to delete room account data", e))?;
        Ok(result.rows_affected() > 0)
    }
}

#[async_trait]
impl RoomAccountDataStoreApi for RoomAccountDataStorage {
    async fn get_room_account_data_content(
        &self,
        user_id: &str,
        room_id: &str,
        data_type: &str,
    ) -> Result<Option<serde_json::Value>, ApiError> {
        self.get_room_account_data_content(user_id, room_id, data_type).await
    }

    async fn get_room_account_data_with_ts(
        &self,
        user_id: &str,
        room_id: &str,
        data_type: &str,
    ) -> Result<Option<(serde_json::Value, Option<i64>)>, ApiError> {
        self.get_room_account_data_with_ts(user_id, room_id, data_type).await
    }

    async fn list_room_account_data(
        &self,
        user_id: &str,
        room_id: &str,
    ) -> Result<Vec<RoomAccountDataRecord>, ApiError> {
        self.list_room_account_data(user_id, room_id).await
    }

    async fn list_room_account_data_batch(
        &self,
        user_id: &str,
        room_ids: &[String],
    ) -> Result<Vec<RoomAccountDataRecord>, ApiError> {
        self.list_room_account_data_batch(user_id, room_ids).await
    }

    async fn upsert_room_account_data(
        &self,
        user_id: &str,
        room_id: &str,
        data_type: &str,
        data: &serde_json::Value,
        now: i64,
    ) -> Result<(), sqlx::Error> {
        self.upsert_room_account_data(user_id, room_id, data_type, data, now).await
    }

    async fn delete_room_account_data(&self, user_id: &str, room_id: &str, data_type: &str) -> Result<bool, ApiError> {
        self.delete_room_account_data(user_id, room_id, data_type).await
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::{Pool, Postgres};
    use synapse_common::current_timestamp_millis;

    /// 每个测试一个从迁移 baseline 克隆出来的独立 schema（返回 guard 与 pool）。
    ///
    /// 2026-09-21：原先用 `connect_shared_test_pool()`（共享 `public`）。共享池的两个问题：
    /// 测试结果取决于环境里 `public` 的残渣（本地 `public` 落后于迁移 baseline 时会直接
    /// 42P01），且并行测试互相影响。按铁律 7 消除共享状态：从模板克隆的 per-test schema
    /// 保证表一定存在、行数从 0 开始（同 `admin_federation.rs`、`event_report/db_tests.rs`）。
    async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<sqlx::PgPool>) {
        let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated test pool");
        let pool = isolated.pool();
        (isolated, pool)
    }

    async fn ensure_test_user(pool: &Pool<Postgres>, user_id: &str) {
        let now = current_timestamp_millis();
        let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
        sqlx::query(
            r#"INSERT INTO users (user_id, username, created_ts)
               VALUES ($1, $2, $3)
               ON CONFLICT (user_id) DO NOTHING"#,
        )
        .bind(user_id)
        .bind(username)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test user");
    }

    async fn ensure_test_room(pool: &Pool<Postgres>, room_id: &str) {
        sqlx::query(
            r#"INSERT INTO rooms (room_id, room_version, is_public, creator, created_ts)
               VALUES ($1, '10', false, $2, $3)
               ON CONFLICT (room_id) DO NOTHING"#,
        )
        .bind(room_id)
        .bind("@test:localhost")
        .bind(current_timestamp_millis())
        .execute(pool)
        .await
        .expect("failed to create test room");
    }

    async fn cleanup_room_account_data(pool: &Pool<Postgres>, user_id: &str, room_id: &str) {
        sqlx::query("DELETE FROM room_account_data WHERE user_id = $1 AND room_id = $2")
            .bind(user_id)
            .bind(room_id)
            .execute(pool)
            .await
            .expect(
                "test fixture: delete must succeed — a swallowed error here surfaces later as an unrelated failure",
            );
    }

    // 1. Store room account data via upsert and verify it exists in the DB.
    #[tokio::test]
    async fn test_store_room_account_data() {
        let (_isolated, pool) = test_pool().await;
        let storage = RoomAccountDataStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@store_{suffix}:localhost");
        let room_id = format!("!store_{suffix}:localhost");

        ensure_test_user(&pool, &user_id).await;
        ensure_test_room(&pool, &room_id).await;
        cleanup_room_account_data(&pool, &user_id, &room_id).await;

        let now = current_timestamp_millis();
        let data = serde_json::json!({"key": "value", "num": 42});

        storage
            .upsert_room_account_data(&user_id, &room_id, "test.type", &data, now)
            .await
            .expect("upsert should succeed");

        let stored = storage
            .get_room_account_data_content(&user_id, &room_id, "test.type")
            .await
            .expect("get should succeed")
            .expect("row should exist");
        assert_eq!(stored, data);

        cleanup_room_account_data(&pool, &user_id, &room_id).await;
    }

    // 2. get_room_account_data_content returns the content for an existing record.
    #[tokio::test]
    async fn test_get_content_found() {
        let (_isolated, pool) = test_pool().await;
        let storage = RoomAccountDataStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@get_found_{suffix}:localhost");
        let room_id = format!("!get_found_{suffix}:localhost");

        ensure_test_user(&pool, &user_id).await;
        ensure_test_room(&pool, &room_id).await;
        cleanup_room_account_data(&pool, &user_id, &room_id).await;

        let now = current_timestamp_millis();
        let data = serde_json::json!({"tag": "m.favourite"});

        storage.upsert_room_account_data(&user_id, &room_id, "m.tag", &data, now).await.expect("upsert should succeed");

        let content = storage
            .get_room_account_data_content(&user_id, &room_id, "m.tag")
            .await
            .expect("get content should succeed")
            .expect("content should exist");

        assert_eq!(content, data);

        cleanup_room_account_data(&pool, &user_id, &room_id).await;
    }

    // 3. get_room_account_data_content returns None for a non-existent record.
    #[tokio::test]
    async fn test_get_content_not_found() {
        let (_isolated, pool) = test_pool().await;
        let storage = RoomAccountDataStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@get_none_{suffix}:localhost");
        let room_id = format!("!get_none_{suffix}:localhost");

        ensure_test_user(&pool, &user_id).await;
        ensure_test_room(&pool, &room_id).await;
        cleanup_room_account_data(&pool, &user_id, &room_id).await;

        let result = storage
            .get_room_account_data_content(&user_id, &room_id, "nonexistent.type")
            .await
            .expect("get content should succeed");

        assert!(result.is_none(), "non-existent data should return None");

        cleanup_room_account_data(&pool, &user_id, &room_id).await;
    }

    // 4. upsert updates existing data for the same (user_id, room_id, data_type) key.
    #[tokio::test]
    async fn test_update_upsert() {
        let (_isolated, pool) = test_pool().await;
        let storage = RoomAccountDataStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@update_{suffix}:localhost");
        let room_id = format!("!update_{suffix}:localhost");

        ensure_test_user(&pool, &user_id).await;
        ensure_test_room(&pool, &room_id).await;
        cleanup_room_account_data(&pool, &user_id, &room_id).await;

        let now = current_timestamp_millis();
        let data_v1 = serde_json::json!({"version": 1});
        let data_v2 = serde_json::json!({"version": 2, "extra": true});

        storage
            .upsert_room_account_data(&user_id, &room_id, "test.update", &data_v1, now)
            .await
            .expect("first upsert should succeed");

        storage
            .upsert_room_account_data(&user_id, &room_id, "test.update", &data_v2, now + 1000)
            .await
            .expect("second upsert should succeed");

        let content = storage
            .get_room_account_data_content(&user_id, &room_id, "test.update")
            .await
            .expect("get content should succeed")
            .expect("content should exist");

        assert_eq!(content, data_v2, "content should reflect the updated value");

        // Verify only one record exists (no duplicate)
        let all = storage.list_room_account_data(&user_id, &room_id).await.expect("list should succeed");
        assert_eq!(all.len(), 1, "should have exactly one record after update");

        cleanup_room_account_data(&pool, &user_id, &room_id).await;
    }

    // 5. delete_room_account_data removes a record and returns true; deleting again returns false.
    #[tokio::test]
    async fn test_delete() {
        let (_isolated, pool) = test_pool().await;
        let storage = RoomAccountDataStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@delete_{suffix}:localhost");
        let room_id = format!("!delete_{suffix}:localhost");

        ensure_test_user(&pool, &user_id).await;
        ensure_test_room(&pool, &room_id).await;
        cleanup_room_account_data(&pool, &user_id, &room_id).await;

        let now = current_timestamp_millis();
        let data = serde_json::json!({"temp": "will-be-deleted"});

        storage
            .upsert_room_account_data(&user_id, &room_id, "test.delete", &data, now)
            .await
            .expect("upsert should succeed");

        let deleted =
            storage.delete_room_account_data(&user_id, &room_id, "test.delete").await.expect("delete should succeed");
        assert!(deleted, "first delete should return true");

        let content = storage
            .get_room_account_data_content(&user_id, &room_id, "test.delete")
            .await
            .expect("get content should succeed");
        assert!(content.is_none(), "data should be gone after delete");

        let deleted_again = storage
            .delete_room_account_data(&user_id, &room_id, "test.delete")
            .await
            .expect("second delete should succeed");
        assert!(!deleted_again, "deleting non-existent record should return false");

        cleanup_room_account_data(&pool, &user_id, &room_id).await;
    }

    // 6. list_room_account_data returns all records for a given user in a given room.
    #[tokio::test]
    async fn test_list_room_account_data() {
        let (_isolated, pool) = test_pool().await;
        let storage = RoomAccountDataStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@list_{suffix}:localhost");
        let room_id = format!("!list_{suffix}:localhost");

        ensure_test_user(&pool, &user_id).await;
        ensure_test_room(&pool, &room_id).await;
        cleanup_room_account_data(&pool, &user_id, &room_id).await;

        let now = current_timestamp_millis();

        storage
            .upsert_room_account_data(&user_id, &room_id, "m.tag", &serde_json::json!({"order": 0.5}), now)
            .await
            .expect("upsert m.tag should succeed");

        storage
            .upsert_room_account_data(&user_id, &room_id, "m.fully_read", &serde_json::json!({"event_id": "$ev1"}), now)
            .await
            .expect("upsert m.fully_read should succeed");

        storage
            .upsert_room_account_data(&user_id, &room_id, "com.example.custom", &serde_json::json!({"a": 1}), now)
            .await
            .expect("upsert custom type should succeed");

        let records = storage.list_room_account_data(&user_id, &room_id).await.expect("list should succeed");

        assert_eq!(records.len(), 3, "should have 3 records");
        assert!(records.iter().all(|r| r.room_id == room_id), "all records should have correct room_id");

        // Verify ordering is ASC by data_type
        let types: Vec<&str> = records.iter().map(|r| r.data_type.as_str()).collect();
        let mut sorted = types.clone();
        sorted.sort();
        assert_eq!(types, sorted, "records should be sorted by data_type ASC");

        cleanup_room_account_data(&pool, &user_id, &room_id).await;
    }

    // 7. list_room_account_data_batch returns data across multiple rooms.
    #[tokio::test]
    async fn test_list_batch() {
        let (_isolated, pool) = test_pool().await;
        let storage = RoomAccountDataStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@batch_{suffix}:localhost");
        let room_a = format!("!batch_a_{suffix}:localhost");
        let room_b = format!("!batch_b_{suffix}:localhost");

        ensure_test_user(&pool, &user_id).await;
        ensure_test_room(&pool, &room_a).await;
        ensure_test_room(&pool, &room_b).await;
        cleanup_room_account_data(&pool, &user_id, &room_a).await;
        cleanup_room_account_data(&pool, &user_id, &room_b).await;

        let now = current_timestamp_millis();

        storage
            .upsert_room_account_data(&user_id, &room_a, "m.tag", &serde_json::json!({"order": 0.1}), now)
            .await
            .expect("upsert room_a");

        storage
            .upsert_room_account_data(&user_id, &room_b, "m.tag", &serde_json::json!({"order": 0.2}), now)
            .await
            .expect("upsert room_b");

        storage
            .upsert_room_account_data(&user_id, &room_b, "com.example.cfg", &serde_json::json!({"x": "y"}), now)
            .await
            .expect("upsert room_b second");

        let room_ids = vec![room_a.clone(), room_b.clone()];
        let records =
            storage.list_room_account_data_batch(&user_id, &room_ids).await.expect("batch list should succeed");

        assert_eq!(records.len(), 3, "should return records from all rooms");

        // Should be ordered by room_id ASC, then data_type ASC
        assert!(records[0].room_id <= records[1].room_id, "records should be sorted by room_id");

        // Empty room_ids list returns empty vec
        let empty: Vec<String> = vec![];
        let no_records =
            storage.list_room_account_data_batch(&user_id, &empty).await.expect("empty batch should succeed");
        assert!(no_records.is_empty(), "empty room_ids should return empty vec");

        cleanup_room_account_data(&pool, &user_id, &room_a).await;
        cleanup_room_account_data(&pool, &user_id, &room_b).await;
    }

    // 8. Round-trip: upsert then verify via get_room_account_data_with_ts including timestamps.
    #[tokio::test]
    async fn test_round_trip_with_ts() {
        let (_isolated, pool) = test_pool().await;
        let storage = RoomAccountDataStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@rtt_{suffix}:localhost");
        let room_id = format!("!rtt_{suffix}:localhost");

        ensure_test_user(&pool, &user_id).await;
        ensure_test_room(&pool, &room_id).await;
        cleanup_room_account_data(&pool, &user_id, &room_id).await;

        let now = current_timestamp_millis();
        let data = serde_json::json!({"name": "round-trip-test", "count": 7});

        storage
            .upsert_room_account_data(&user_id, &room_id, "com.test.roundtrip", &data, now)
            .await
            .expect("upsert should succeed");

        let (content, updated_ts) = storage
            .get_room_account_data_with_ts(&user_id, &room_id, "com.test.roundtrip")
            .await
            .expect("get with ts should succeed")
            .expect("record should exist");

        assert_eq!(content, data, "content should match");
        assert!(updated_ts.is_some(), "updated_ts should be present");
        assert_eq!(updated_ts.unwrap(), now, "updated_ts should match the upsert timestamp");

        // Querying a non-existent record with get_room_account_data_with_ts returns None
        let result = storage
            .get_room_account_data_with_ts(&user_id, &room_id, "com.test.nonexistent")
            .await
            .expect("get nonexistent with ts should succeed");
        assert!(result.is_none(), "non-existent record should return None");

        cleanup_room_account_data(&pool, &user_id, &room_id).await;
    }
}

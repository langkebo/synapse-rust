use super::models::{KeyRequestInfo, KeyRequestPagination};
use sqlx::PgPool;
use synapse_common::current_timestamp_millis;
use synapse_common::map_database;
use synapse_common::ApiError;

#[derive(Clone)]
/// The `KeyRequestStorage` type.
pub struct KeyRequestStorage {
    pool: PgPool,
}

/// Implementation of [`KeyRequestStorage`] methods.
impl KeyRequestStorage {
    /// See [`new`].
    pub fn new(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }

    /// See [`create_request`].
    pub async fn create_request(&self, request: &KeyRequestInfo) -> Result<(), ApiError> {
        sqlx::query(
            r"
            INSERT INTO e2ee_key_requests
                (request_id, user_id, device_id, room_id, session_id, algorithm, action, created_ts, is_fulfilled)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (request_id) DO UPDATE SET
                action = EXCLUDED.action,
                is_fulfilled = EXCLUDED.is_fulfilled
            ",
        )
        .bind(&request.request_id)
        .bind(&request.user_id)
        .bind(&request.device_id)
        .bind(&request.room_id)
        .bind(&request.session_id)
        .bind(&request.algorithm)
        .bind(&request.action)
        .bind(request.created_ts)
        .bind(request.is_fulfilled)
        .execute(&self.pool)
        .await
        .map_err(map_database!("create_request"))?;

        Ok(())
    }

    /// See [`get_request`].
    pub async fn get_request(&self, request_id: &str) -> Result<Option<KeyRequestInfo>, ApiError> {
        sqlx::query_as::<_, KeyRequestInfo>(
            r"
            SELECT
                request_id,
                user_id,
                device_id,
                room_id,
                session_id,
                algorithm,
                action,
                created_ts,
                COALESCE(is_fulfilled, FALSE) AS is_fulfilled,
                fulfilled_by_device,
                fulfilled_ts
            FROM e2ee_key_requests
            WHERE request_id = $1
            ",
        )
        .bind(request_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_database!("get_request"))
    }

    /// See [`get_requests_for_user`].
    pub async fn get_requests_for_user(&self, user_id: &str) -> Result<Vec<KeyRequestInfo>, ApiError> {
        sqlx::query_as::<_, KeyRequestInfo>(
            r"
            SELECT
                request_id,
                user_id,
                device_id,
                room_id,
                session_id,
                algorithm,
                action,
                created_ts,
                COALESCE(is_fulfilled, FALSE) AS is_fulfilled,
                fulfilled_by_device,
                fulfilled_ts
            FROM e2ee_key_requests
            WHERE user_id = $1
            ORDER BY created_ts DESC
            LIMIT 100
            ",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_database!("get_requests_for_user"))
    }

    /// See [`get_all_pending_requests`].
    pub async fn get_all_pending_requests(&self) -> Result<Vec<KeyRequestInfo>, ApiError> {
        sqlx::query_as::<_, KeyRequestInfo>(
            r"
            SELECT
                request_id,
                user_id,
                device_id,
                room_id,
                session_id,
                algorithm,
                action,
                created_ts,
                COALESCE(is_fulfilled, FALSE) AS is_fulfilled,
                fulfilled_by_device,
                fulfilled_ts
            FROM e2ee_key_requests
            WHERE is_fulfilled = FALSE
            ORDER BY created_ts DESC
            LIMIT 100
            ",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_database!("get_all_pending_requests"))
    }

    /// See [`fulfill_request`].
    pub async fn fulfill_request(&self, request_id: &str, device_id: &str) -> Result<(), ApiError> {
        let now = current_timestamp_millis();

        sqlx::query(
            r"
            UPDATE e2ee_key_requests
            SET is_fulfilled = TRUE, fulfilled_by_device = $2, fulfilled_ts = $3
            WHERE request_id = $1
            ",
        )
        .bind(request_id)
        .bind(device_id)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(map_database!("fulfill_request"))?;

        Ok(())
    }

    /// See [`cancel_request`].
    pub async fn cancel_request(&self, request_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            r"
            UPDATE e2ee_key_requests
            SET action = 'cancellation', is_fulfilled = TRUE
            WHERE request_id = $1
            ",
        )
        .bind(request_id)
        .execute(&self.pool)
        .await
        .map_err(map_database!("cancel_request"))?;

        Ok(())
    }

    /// See [`update_request_status`].
    pub async fn update_request_status(&self, request_id: &str, status: &str) -> Result<(), ApiError> {
        let now = current_timestamp_millis();

        sqlx::query(
            r"
            UPDATE e2ee_key_requests
            SET action = $2, updated_ts = $3
            WHERE request_id = $1
            ",
        )
        .bind(request_id)
        .bind(status)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(map_database!("update_request_status"))?;

        Ok(())
    }

    /// See [`delete_request`].
    pub async fn delete_request(&self, request_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            r"
            DELETE FROM e2ee_key_requests WHERE request_id = $1
            ",
        )
        .bind(request_id)
        .execute(&self.pool)
        .await
        .map_err(map_database!("delete_request"))?;

        Ok(())
    }

    /// See [`delete_old_requests`].
    pub async fn delete_old_requests(&self, older_than_ts: i64) -> Result<u64, ApiError> {
        let result = sqlx::query(
            r"
            DELETE FROM e2ee_key_requests
            WHERE is_fulfilled = TRUE AND fulfilled_ts < $1
            ",
        )
        .bind(older_than_ts)
        .execute(&self.pool)
        .await
        .map_err(map_database!("delete_old_requests"))?;

        Ok(result.rows_affected())
    }

    /// See [`get_requests_paginated`].
    pub async fn get_requests_paginated(
        &self,
        pagination: KeyRequestPagination<'_>,
    ) -> Result<Vec<KeyRequestInfo>, ApiError> {
        let KeyRequestPagination { user_id, limit, from_ts, from_id, status, room_id, session_id } = pagination;
        let mut query = sqlx::QueryBuilder::new(
            r#"
            SELECT
                request_id,
                user_id,
                device_id,
                room_id,
                session_id,
                algorithm,
                action,
                created_ts,
                COALESCE(is_fulfilled, FALSE) AS is_fulfilled,
                fulfilled_by_device,
                fulfilled_ts
            FROM e2ee_key_requests
            WHERE user_id = "#,
        );
        query.push_bind(user_id);

        if let Some(room) = room_id {
            query.push(" AND room_id = ");
            query.push_bind(room);
        }

        if let Some(session) = session_id {
            query.push(" AND session_id = ");
            query.push_bind(session);
        }

        if let Some(status) = status {
            match status {
                "pending" => {
                    query.push(" AND is_fulfilled = FALSE");
                }
                "fulfilled" => {
                    query.push(" AND is_fulfilled = TRUE");
                }
                "cancelled" => {
                    query.push(" AND (action = 'cancelled' OR action = 'cancellation')");
                }
                _ => {}
            }
        }

        if let (Some(ts), Some(id)) = (from_ts, from_id) {
            query.push(" AND (created_ts < ");
            query.push_bind(ts);
            query.push(" OR (created_ts = ");
            query.push_bind(ts);
            query.push(" AND request_id < ");
            query.push_bind(id);
            query.push("))");
        }

        query.push(" ORDER BY created_ts DESC, request_id DESC LIMIT ");
        query.push_bind(limit);

        query
            .build_query_as::<KeyRequestInfo>()
            .fetch_all(&self.pool)
            .await
            .map_err(map_database!("get_requests_paginated"))
    }
}

// ---------------------------------------------------------------------------
// C39-0：C39 转换前补齐零覆盖路径
//
// 背景：`key_request/storage.rs` 的 9 处字面量动态 SQL 在 `tests/` 里**零引用**，
// 也没有 `db_tests`（与 `olm/` `megolm/` `backup/` 三个同域模块不同）。
// 直接转换会重演 D-15.6（宏只证明"能 describe"，证不了"行为没变"）⇒ 先把 9 条路径补齐。
// 池一律 `IsolatedTestPool::new(BASELINE_SQL)`（真 baseline schema，R9）。
// ---------------------------------------------------------------------------
#[cfg(test)]
mod db_tests {
    use super::*;
    use synapse_common::test_isolation::IsolatedTestPool;

    /// 工作区 baseline 迁移，编译期读入，使隔离 schema 就是真 schema。
    ///
    /// ⚠️ 字节是**载荷**：共享模板名是该字符串内容的指纹，必须与
    /// `synapse-storage/src/test_isolation.rs`、`synapse-e2ee/src/backup/storage.rs`
    /// 和 `synapse-services/src/test_utils.rs` 里的副本逐字节一致。
    const BASELINE_SQL: &str = include_str!("../../../migrations/00000000_unified_schema_v12.sql");

    async fn test_pool() -> (IsolatedTestPool, PgPool) {
        let isolated = IsolatedTestPool::new(BASELINE_SQL).await.expect("isolated baseline test pool");
        let pool = (*isolated.pool()).clone();
        (isolated, pool)
    }

    fn make_request(user_id: &str, request_id: &str, created_ts: i64, action: &str) -> KeyRequestInfo {
        KeyRequestInfo {
            request_id: request_id.to_string(),
            user_id: user_id.to_string(),
            device_id: "DEV1".to_string(),
            room_id: "!kr_room:example.com".to_string(),
            session_id: "sess1".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            action: action.to_string(),
            created_ts,
            is_fulfilled: false,
            fulfilled_by_device: None,
            fulfilled_ts: None,
        }
    }

    /// create → get：字段往返（含 `is_fulfilled` 的 `COALESCE(is_fulfilled, FALSE)` 投影），
    /// 未知 request_id 返回 `None`。
    #[tokio::test]
    async fn test_create_and_get_request_round_trip() {
        let (_isolated, pool) = test_pool().await;
        let storage = KeyRequestStorage::new(&pool);

        assert!(storage.get_request("missing").await.expect("get missing").is_none(), "未知 request_id 必须是 None");

        let request = make_request("@alice:example.com", "req-1", 1000, "request");
        storage.create_request(&request).await.expect("create_request");

        let fetched = storage.get_request("req-1").await.expect("get_request").expect("row must exist");
        assert_eq!(fetched.request_id, "req-1");
        assert_eq!(fetched.user_id, "@alice:example.com");
        assert_eq!(fetched.device_id, "DEV1");
        assert_eq!(fetched.room_id, "!kr_room:example.com");
        assert_eq!(fetched.session_id, "sess1");
        assert_eq!(fetched.algorithm, "m.megolm.v1.aes-sha2");
        assert_eq!(fetched.action, "request");
        assert_eq!(fetched.created_ts, 1000);
        assert!(!fetched.is_fulfilled, "新请求必须是未完成");
        assert_eq!(fetched.fulfilled_by_device, None);
        assert_eq!(fetched.fulfilled_ts, None);
    }

    /// `create_request` 对同一 `request_id` 是 upsert：只覆盖 `action` 与 `is_fulfilled`，
    /// 不新增行。
    #[tokio::test]
    async fn test_create_request_upserts_on_request_id() {
        let (_isolated, pool) = test_pool().await;
        let storage = KeyRequestStorage::new(&pool);

        storage
            .create_request(&make_request("@alice:example.com", "req-dup", 1000, "request"))
            .await
            .expect("first create");
        let mut second = make_request("@alice:example.com", "req-dup", 2000, "cancellation");
        second.is_fulfilled = true;
        storage.create_request(&second).await.expect("upsert");

        let all = storage.get_requests_for_user("@alice:example.com").await.expect("list");
        assert_eq!(all.len(), 1, "同一 request_id 只允许一行");
        assert_eq!(all[0].action, "cancellation", "action 必须被覆盖");
        assert!(all[0].is_fulfilled, "is_fulfilled 必须被覆盖");
    }

    /// `get_requests_for_user` 按用户收敛并按 `created_ts DESC` 排序；
    /// `get_all_pending_requests` 只返回未完成的行。
    #[tokio::test]
    async fn test_user_scoping_pending_filter_and_ordering() {
        let (_isolated, pool) = test_pool().await;
        let storage = KeyRequestStorage::new(&pool);

        storage.create_request(&make_request("@alice:example.com", "req-a1", 1000, "request")).await.unwrap();
        storage.create_request(&make_request("@alice:example.com", "req-a2", 3000, "request")).await.unwrap();
        storage.create_request(&make_request("@alice:example.com", "req-a3", 2000, "request")).await.unwrap();
        storage.create_request(&make_request("@bob:example.com", "req-b1", 1500, "request")).await.unwrap();

        let alice = storage.get_requests_for_user("@alice:example.com").await.expect("alice");
        let ids: Vec<&str> = alice.iter().map(|r| r.request_id.as_str()).collect();
        assert_eq!(ids, vec!["req-a2", "req-a3", "req-a1"], "必须按 created_ts DESC 且只看该用户");

        let bob = storage.get_requests_for_user("@bob:example.com").await.expect("bob");
        assert_eq!(bob.len(), 1);
        assert_eq!(bob[0].request_id, "req-b1");

        storage.fulfill_request("req-a2", "ANSWER_DEV").await.expect("fulfill");
        let pending = storage.get_all_pending_requests().await.expect("pending");
        let pending_ids: Vec<&str> = pending.iter().map(|r| r.request_id.as_str()).collect();
        assert!(!pending_ids.contains(&"req-a2"), "已完成的行不得出现在 pending 里");
        assert_eq!(pending_ids, vec!["req-a3", "req-b1", "req-a1"], "其余按 created_ts DESC");
    }

    /// `fulfill_request` / `cancel_request` / `update_request_status` 各自的写入语义。
    #[tokio::test]
    async fn test_fulfill_cancel_and_update_status_write_expected_columns() {
        let (_isolated, pool) = test_pool().await;
        let storage = KeyRequestStorage::new(&pool);

        storage.create_request(&make_request("@alice:example.com", "req-f", 1000, "request")).await.unwrap();
        storage.fulfill_request("req-f", "ANSWER_DEV").await.expect("fulfill");
        let fulfilled = storage.get_request("req-f").await.unwrap().unwrap();
        assert!(fulfilled.is_fulfilled);
        assert_eq!(fulfilled.fulfilled_by_device.as_deref(), Some("ANSWER_DEV"));
        assert!(fulfilled.fulfilled_ts.is_some(), "fulfilled_ts 必须被写入");

        storage.create_request(&make_request("@alice:example.com", "req-c", 2000, "request")).await.unwrap();
        storage.cancel_request("req-c").await.expect("cancel");
        let cancelled = storage.get_request("req-c").await.unwrap().unwrap();
        assert_eq!(cancelled.action, "cancellation");
        assert!(cancelled.is_fulfilled, "cancel 也把 is_fulfilled 置真");

        storage.update_request_status("req-c", "rejected").await.expect("update status");
        let updated = storage.get_request("req-c").await.unwrap().unwrap();
        assert_eq!(updated.action, "rejected");
    }

    /// `delete_request` 按 id 删；`delete_old_requests` 只删**已完成且 fulfilled_ts 早于阈值**的行。
    #[tokio::test]
    async fn test_delete_request_and_delete_old_requests() {
        let (_isolated, pool) = test_pool().await;
        let storage = KeyRequestStorage::new(&pool);

        storage.create_request(&make_request("@alice:example.com", "req-del", 1000, "request")).await.unwrap();
        storage.delete_request("req-del").await.expect("delete_request");
        assert!(storage.get_request("req-del").await.unwrap().is_none(), "delete_request 必须真的删掉");

        // 已完成（fulfilled_ts = now）与未完成各一条；阈值取一个远未来 ⇒ 只有已完成那条会被删。
        storage.create_request(&make_request("@alice:example.com", "req-old", 1000, "request")).await.unwrap();
        storage.fulfill_request("req-old", "ANSWER_DEV").await.expect("fulfill");
        storage.create_request(&make_request("@alice:example.com", "req-pending", 1000, "request")).await.unwrap();

        let deleted = storage.delete_old_requests(i64::MAX).await.expect("delete_old_requests");
        assert_eq!(deleted, 1, "只应删掉已完成的那条");
        assert!(storage.get_request("req-old").await.unwrap().is_none());
        assert!(storage.get_request("req-pending").await.unwrap().is_some(), "未完成的行必须保留");
    }
}

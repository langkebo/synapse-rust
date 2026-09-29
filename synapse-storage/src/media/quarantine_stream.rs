use crate::media::models::QuarantinedMediaChange;
use async_trait::async_trait;
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::ApiError;

/// Store API for quarantined media change tracking.
///
/// Provides methods to record quarantine/unquarantine changes and query
/// incremental changes for stream replication between workers.
#[async_trait]
pub trait QuarantinedMediaChangeStoreApi: Send + Sync {
    /// See [`record_media_quarantine_change`].
    async fn record_media_quarantine_change(
        &self,
        media_id: &str,
        server_name: &str,
        change_type: &str,
        changed_by: &str,
        now_ts: i64,
    ) -> Result<i64, ApiError>;

    /// See [`get_quarantined_media_changes`].
    async fn get_quarantined_media_changes(
        &self,
        since_stream_id: i64,
        limit: i64,
    ) -> Result<Vec<QuarantinedMediaChange>, ApiError>;

    /// Query quarantine changes filtered by `media_id`, for the
    /// `GET /_synapse/admin/v1/quarantine_media/{media_id}/changes` admin
    /// endpoint. Returns changes with `stream_id > since_stream_id`, ordered
    /// ascending, capped by `limit`.
    async fn get_changes_by_media(
        &self,
        media_id: &str,
        since_stream_id: i64,
        limit: i64,
    ) -> Result<Vec<QuarantinedMediaChange>, ApiError>;

    /// See [`set_media_quarantine_status`].
    async fn set_media_quarantine_status(
        &self,
        media_id: &str,
        server_name: &str,
        quarantine_status: &str,
    ) -> Result<bool, ApiError>;

    /// Check if media is currently quarantined.
    /// Returns `Ok(true)` if `quarantine_status` is `"quarantined"`, `Ok(false)`
    /// otherwise (including when the media row is missing).
    async fn get_media_quarantine_status(&self, media_id: &str, server_name: &str) -> Result<bool, ApiError>;

    /// See [`get_current_stream_id`].
    async fn get_current_stream_id(&self) -> Result<i64, ApiError>;
}

/// Storage layer for the `quarantined_media_changes` stream table.
///
/// Provides methods to record quarantine/unquarantine changes and query
/// incremental changes for stream replication between workers.
#[derive(Clone)]
pub struct QuarantinedMediaChangeStorage {
    pool: PgPool,
}

impl QuarantinedMediaChangeStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: (**pool).clone() }
    }

    /// Record a media quarantine/unquarantine change and return the new stream_id.
    pub async fn record_media_quarantine_change(
        &self,
        media_id: &str,
        server_name: &str,
        change_type: &str,
        changed_by: &str,
        now_ts: i64,
    ) -> Result<i64, ApiError> {
        // 六列 `RETURNING` 与 `QuarantinedMediaChange` 的字段一一对应（R6 ⑤；列清单本就显式）。
        let row = sqlx::query_as!(
            QuarantinedMediaChange,
            r#"
            INSERT INTO quarantined_media_changes (media_id, server_name, change_type, changed_by, created_ts)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING stream_id, media_id, server_name, change_type, changed_by, created_ts
            "#,
            media_id,
            server_name,
            change_type,
            changed_by,
            now_ts
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to record media quarantine change", e))?;

        Ok(row.stream_id)
    }

    /// Get incremental quarantine changes since the given stream_id.
    pub async fn get_quarantined_media_changes(
        &self,
        since_stream_id: i64,
        limit: i64,
    ) -> Result<Vec<QuarantinedMediaChange>, ApiError> {
        let changes = sqlx::query_as!(
            QuarantinedMediaChange,
            r#"
            SELECT stream_id, media_id, server_name, change_type, changed_by, created_ts
            FROM quarantined_media_changes
            WHERE stream_id > $1
            ORDER BY stream_id ASC
            LIMIT $2
            "#,
            since_stream_id,
            limit
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get quarantined media changes", e))?;

        Ok(changes)
    }

    /// Get quarantine changes for a specific media_id since the given
    /// stream_id. Backs the `GET /_synapse/admin/v1/quarantine_media/{media_id}/changes`
    /// admin endpoint.
    pub async fn get_changes_by_media(
        &self,
        media_id: &str,
        since_stream_id: i64,
        limit: i64,
    ) -> Result<Vec<QuarantinedMediaChange>, ApiError> {
        let changes = sqlx::query_as!(
            QuarantinedMediaChange,
            r#"
            SELECT stream_id, media_id, server_name, change_type, changed_by, created_ts
            FROM quarantined_media_changes
            WHERE media_id = $1 AND stream_id > $2
            ORDER BY stream_id ASC
            LIMIT $3
            "#,
            media_id,
            since_stream_id,
            limit
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to get quarantined media changes by media_id", e))?;

        Ok(changes)
    }

    /// Update the quarantine_status column on media_metadata.
    pub async fn set_media_quarantine_status(
        &self,
        media_id: &str,
        server_name: &str,
        quarantine_status: &str,
    ) -> Result<bool, ApiError> {
        let result = sqlx::query!(
            r#"
            UPDATE media_metadata
            SET quarantine_status = $1
            WHERE media_id = $2 AND server_name = $3
            "#,
            quarantine_status,
            media_id,
            server_name
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to update media quarantine status", e))?;

        Ok(result.rows_affected() > 0)
    }

    /// Check whether the media row's `quarantine_status` column equals
    /// `"quarantined"`. Returns `Ok(false)` when the media row does not exist
    /// (download path will surface its own 404).
    pub async fn get_media_quarantine_status(&self, media_id: &str, server_name: &str) -> Result<bool, ApiError> {
        // 可空列（`media_metadata.quarantine_status` 是 `TEXT` 无 NOT NULL）+ `fetch_optional`
        // ⇒ `Option<Option<String>>` 两层（R6 ②）。
        let status: Option<Option<String>> = sqlx::query_scalar!(
            r#"
            SELECT quarantine_status
            FROM media_metadata
            WHERE media_id = $1 AND server_name = $2
            "#,
            media_id,
            server_name
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to query media quarantine status", e))?;

        Ok(matches!(status, Some(Some(ref s)) if s == "quarantined"))
    }

    /// Get the current maximum stream_id (used for position tracking).
    pub async fn get_current_stream_id(&self) -> Result<i64, ApiError> {
        // `MAX(...)` 是聚合、无关系来源 ⇒ 宏推 `Option<i64>`（空表返回 NULL 是**正常**语义，
        // 与"吞掉 DB 错误"无关：`Result` 仍由 `?` 传播）。`unwrap_or(0)` 只作用于那个可空值。
        let stream_id: Option<i64> = sqlx::query_scalar!(r"SELECT MAX(stream_id) FROM quarantined_media_changes")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get current quarantine stream id", e))?;

        Ok(stream_id.unwrap_or(0))
    }
}

#[async_trait]
impl QuarantinedMediaChangeStoreApi for QuarantinedMediaChangeStorage {
    async fn record_media_quarantine_change(
        &self,
        media_id: &str,
        server_name: &str,
        change_type: &str,
        changed_by: &str,
        now_ts: i64,
    ) -> Result<i64, ApiError> {
        self.record_media_quarantine_change(media_id, server_name, change_type, changed_by, now_ts).await
    }

    async fn get_quarantined_media_changes(
        &self,
        since_stream_id: i64,
        limit: i64,
    ) -> Result<Vec<QuarantinedMediaChange>, ApiError> {
        self.get_quarantined_media_changes(since_stream_id, limit).await
    }

    async fn get_changes_by_media(
        &self,
        media_id: &str,
        since_stream_id: i64,
        limit: i64,
    ) -> Result<Vec<QuarantinedMediaChange>, ApiError> {
        self.get_changes_by_media(media_id, since_stream_id, limit).await
    }

    async fn set_media_quarantine_status(
        &self,
        media_id: &str,
        server_name: &str,
        quarantine_status: &str,
    ) -> Result<bool, ApiError> {
        self.set_media_quarantine_status(media_id, server_name, quarantine_status).await
    }

    async fn get_media_quarantine_status(&self, media_id: &str, server_name: &str) -> Result<bool, ApiError> {
        self.get_media_quarantine_status(media_id, server_name).await
    }

    async fn get_current_stream_id(&self) -> Result<i64, ApiError> {
        self.get_current_stream_id().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_creation() {
        // Verify the storage struct can be constructed (pool connectivity tested in integration tests)
        let _model = QuarantinedMediaChange {
            stream_id: 1,
            media_id: "abc123".to_string(),
            server_name: "example.com".to_string(),
            change_type: "quarantine".to_string(),
            changed_by: "@admin:example.com".to_string(),
            created_ts: 1234567890000,
        };
    }
}

/// `QuarantinedMediaChangeStorage` 的**真 baseline** 往返覆盖（C52-0）。
///
/// 本文件此前只有一个"能构造结构体"的纯单测，6 个方法**零 DB 覆盖**；而
/// `quarantined_media_changes` 的六列全是 NOT NULL、`media_metadata.quarantine_status` 可空
/// —— 正是宏转换最容易搞错可空性的形状。所有用例跑在 `isolated_test_pool()` 的 per-test
/// schema 上（R9），不碰共享 `public`。
#[cfg(test)]
mod db_tests {
    use super::*;

    async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<PgPool>) {
        let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
        let pool = isolated.pool();
        (isolated, pool)
    }

    /// `media_metadata` 的必填列（`file_name` / `uploader_user_id` / `quarantine_status` 等可空）。
    async fn insert_media(pool: &PgPool, media_id: &str, server_name: &str, quarantine_status: Option<&str>) {
        sqlx::query(
            "INSERT INTO media_metadata (media_id, server_name, content_type, size, created_ts, quarantine_status) \
             VALUES ($1, $2, 'image/png', 123, 1000, $3)",
        )
        .bind(media_id)
        .bind(server_name)
        .bind(quarantine_status)
        .execute(pool)
        .await
        .expect("insert media_metadata");
    }

    #[tokio::test]
    async fn quarantine_stream_lifecycle_round_trip_on_the_migration_template() {
        let (isolated, pool) = test_pool().await;
        let storage = QuarantinedMediaChangeStorage::new(&pool);
        let server = "media.example.com";

        // 空表 ⇒ `get_current_stream_id` 为 0（MAX 无行 ⇒ NULL ⇒ unwrap_or(0)）。
        assert_eq!(storage.get_current_stream_id().await.unwrap(), 0);
        assert!(storage.get_quarantined_media_changes(0, 10).await.unwrap().is_empty());

        // record：返回的 stream_id 必须严格递增（BIGSERIAL）。
        let first =
            storage.record_media_quarantine_change("m1", server, "quarantine", "@admin:test", 1000).await.unwrap();
        let second =
            storage.record_media_quarantine_change("m1", server, "unquarantine", "@admin:test", 2000).await.unwrap();
        let third =
            storage.record_media_quarantine_change("m2", server, "quarantine", "@admin:test", 3000).await.unwrap();
        assert!(first < second && second < third, "stream_id 必须严格递增: {first} {second} {third}");

        // get_quarantined_media_changes：`> since`、升序、LIMIT 生效。
        let all = storage.get_quarantined_media_changes(0, 10).await.unwrap();
        assert_eq!(all.iter().map(|c| c.stream_id).collect::<Vec<_>>(), vec![first, second, third]);
        assert_eq!(all[0].media_id, "m1");
        assert_eq!(all[0].change_type, "quarantine");
        assert_eq!(all[0].changed_by, "@admin:test");
        assert_eq!(all[0].created_ts, 1000);
        let after_first = storage.get_quarantined_media_changes(first, 10).await.unwrap();
        assert_eq!(after_first.iter().map(|c| c.stream_id).collect::<Vec<_>>(), vec![second, third]);
        assert_eq!(storage.get_quarantined_media_changes(0, 1).await.unwrap().len(), 1);

        // get_changes_by_media：按 media_id 过滤，且仍受 since / LIMIT 约束。
        let m1 = storage.get_changes_by_media("m1", 0, 10).await.unwrap();
        assert_eq!(m1.iter().map(|c| c.stream_id).collect::<Vec<_>>(), vec![first, second]);
        assert_eq!(storage.get_changes_by_media("m1", first, 10).await.unwrap().len(), 1);
        assert!(storage.get_changes_by_media("unknown", 0, 10).await.unwrap().is_empty());

        // get_current_stream_id ⇒ 当前最大 stream_id。
        assert_eq!(storage.get_current_stream_id().await.unwrap(), third);

        // media_metadata 侧：可空列的两种状态 + 不存在行的语义。
        insert_media(&pool, "m1", server, None).await;
        insert_media(&pool, "m2", server, Some("quarantined")).await;
        assert!(!storage.get_media_quarantine_status("m1", server).await.unwrap(), "NULL ⇒ false");
        assert!(storage.get_media_quarantine_status("m2", server).await.unwrap(), "'quarantined' ⇒ true");
        assert!(
            !storage.get_media_quarantine_status("missing", server).await.unwrap(),
            "缺行 ⇒ false（调用方自己 404）"
        );

        // set_media_quarantine_status：命中 ⇒ true（且再次调用仍 true，更新是幂等的），缺行 ⇒ false。
        assert!(storage.set_media_quarantine_status("m1", server, "quarantined").await.unwrap());
        assert!(storage.get_media_quarantine_status("m1", server).await.unwrap());
        assert!(storage.set_media_quarantine_status("m1", server, "quarantined").await.unwrap());
        assert!(storage.set_media_quarantine_status("m1", server, "clean").await.unwrap());
        assert!(!storage.get_media_quarantine_status("m1", server).await.unwrap(), "'clean' ⇒ false");
        assert!(!storage.set_media_quarantine_status("missing", server, "quarantined").await.unwrap());

        drop(isolated);
    }
}

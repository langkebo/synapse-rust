use super::models::*;
use crate::error::ApiError;
use chrono::Utc;
use sqlx::PgPool;
use std::str::FromStr;
use std::sync::Arc;

/// Internal row struct for `megolm_sessions` (matches DB column types exactly,
/// including BIGINT timestamps that the public model converts to DateTime<Utc>).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MegolmSessionRow {
    pub id: uuid::Uuid,
    pub session_id: String,
    pub room_id: String,
    pub sender_key: String,
    pub session_key: String,
    pub algorithm: String,
    pub message_index: i64,
    pub created_ts: i64,
    pub last_used_ts: Option<i64>,
    pub expires_at: Option<i64>,
    pub pickle_format: String,
    pub vodozemac_pickle: Option<String>,
}

impl From<MegolmSessionRow> for MegolmSession {
    fn from(row: MegolmSessionRow) -> Self {
        let created_ts_dt = chrono::DateTime::from_timestamp_millis(row.created_ts).unwrap_or_else(Utc::now);
        let last_used_ts_dt =
            row.last_used_ts.and_then(chrono::DateTime::from_timestamp_millis).unwrap_or(created_ts_dt);
        let expires_at_dt = row.expires_at.and_then(chrono::DateTime::from_timestamp_millis);

        MegolmSession {
            id: row.id,
            session_id: row.session_id,
            room_id: row.room_id,
            sender_key: row.sender_key,
            session_key: row.session_key,
            algorithm: row.algorithm,
            message_index: row.message_index,
            created_ts: created_ts_dt,
            last_used_ts: last_used_ts_dt,
            expires_at: expires_at_dt,
            pickle_format: PickleFormat::from_str(&row.pickle_format).unwrap_or(PickleFormat::Legacy),
            vodozemac_pickle: row.vodozemac_pickle,
        }
    }
}

#[derive(Clone)]
pub struct MegolmSessionStorage {
    pub pool: Arc<PgPool>,
}

impl MegolmSessionStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_session(&self, session: &MegolmSession) -> Result<(), ApiError> {
        sqlx::query!(
            r"
            INSERT INTO megolm_sessions (
                id, session_id, room_id, sender_key, session_key, algorithm,
                message_index, created_ts, last_used_ts, expires_at,
                pickle_format, vodozemac_pickle
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ",
            session.id,
            session.session_id,
            session.room_id,
            session.sender_key,
            session.session_key,
            session.algorithm,
            session.message_index,
            session.created_ts.timestamp_millis(),
            session.last_used_ts.timestamp_millis(),
            session.expires_at.map(|t| t.timestamp_millis()),
            session.pickle_format.as_str(),
            session.vodozemac_pickle.as_deref(),
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create megolm session: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Option<MegolmSession>, ApiError> {
        let row: Option<MegolmSessionRow> = sqlx::query_as!(
            MegolmSessionRow,
            r#"
            SELECT
                id AS "id!",
                session_id AS "session_id!",
                room_id AS "room_id!",
                sender_key AS "sender_key!",
                session_key AS "session_key!",
                algorithm AS "algorithm!",
                message_index AS "message_index!",
                created_ts AS "created_ts!",
                last_used_ts AS "last_used_ts?",
                expires_at AS "expires_at?",
                pickle_format AS "pickle_format!",
                vodozemac_pickle AS "vodozemac_pickle?"
            FROM megolm_sessions
            WHERE session_id = $1
            "#,
            session_id,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load megolm session: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(row.map(Into::into))
    }

    pub async fn get_room_sessions(&self, room_id: &str) -> Result<Vec<MegolmSession>, ApiError> {
        let rows: Vec<MegolmSessionRow> = sqlx::query_as!(
            MegolmSessionRow,
            r#"
            SELECT
                id AS "id!",
                session_id AS "session_id!",
                room_id AS "room_id!",
                sender_key AS "sender_key!",
                session_key AS "session_key!",
                algorithm AS "algorithm!",
                message_index AS "message_index!",
                created_ts AS "created_ts!",
                last_used_ts AS "last_used_ts?",
                expires_at AS "expires_at?",
                pickle_format AS "pickle_format!",
                vodozemac_pickle AS "vodozemac_pickle?"
            FROM megolm_sessions
            WHERE room_id = $1
            "#,
            room_id,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load megolm sessions: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn update_session(&self, session: &MegolmSession) -> Result<(), ApiError> {
        sqlx::query!(
            r"
            UPDATE megolm_sessions
            SET session_key = $2,
                message_index = $3,
                last_used_ts = $4,
                expires_at = $5,
                pickle_format = $6,
                vodozemac_pickle = $7
            WHERE session_id = $1
            ",
            session.session_id,
            session.session_key,
            session.message_index,
            session.last_used_ts.timestamp_millis(),
            session.expires_at.map(|t| t.timestamp_millis()),
            session.pickle_format.as_str(),
            session.vodozemac_pickle.as_deref(),
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update megolm session: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;
        Ok(())
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<(), ApiError> {
        sqlx::query!(
            r"
            DELETE FROM megolm_sessions
            WHERE session_id = $1
            ",
            session_id,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete megolm session: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    // ========================================================================
    // vodozemac Megolm 路径（Phase 1）— 由 MegolmVodozemacService 调用
    // ========================================================================

    /// 原子地增加 message_index 并返回新值。
    /// vodozemac 路径下加密 N 条消息时使用，避免并发加密撞索引。
    pub async fn increment_message_index(
        &self,
        session_id: &str,
        delta: i64,
        now_ms: i64,
    ) -> Result<Option<i64>, ApiError> {
        let row: Option<MegolmIncrementRow> = sqlx::query_as!(
            MegolmIncrementRow,
            r#"
            UPDATE megolm_sessions
            SET message_index = message_index + $2,
                last_used_ts = $3
            WHERE session_id = $1
            RETURNING message_index AS "message_index!"
            "#,
            session_id,
            delta,
            now_ms,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to increment megolm message index: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(row.map(|r| r.message_index))
    }

    /// 更新 vodozemac pickle 副本（Phase 2 双写：encrypt/decrypt 后持久化新 ratchet state）
    ///
    /// 同时刷新 `last_used_ts` 便于监控。
    pub async fn update_vodozemac_pickle(
        &self,
        session_id: &str,
        vodozemac_pickle: &str,
        now_ms: i64,
    ) -> Result<bool, ApiError> {
        let result = sqlx::query!(
            r"
            UPDATE megolm_sessions
            SET vodozemac_pickle = $2,
                last_used_ts = $3
            WHERE session_id = $1
            ",
            session_id,
            vodozemac_pickle,
            now_ms,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update vodozemac pickle: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(result.rows_affected() > 0)
    }

    /// 批量 upsert session keys（向多个用户共享 session_key 时使用）
    pub async fn upsert_session_keys_batch(
        &self,
        user_ids: &[String],
        session_id: &str,
        encrypted_key: &str,
        created_ts: i64,
        expires_at: Option<i64>,
    ) -> Result<u64, ApiError> {
        if user_ids.is_empty() {
            return Ok(0);
        }

        let mut total: u64 = 0;
        for user_id in user_ids {
            let result = sqlx::query!(
                r"
                INSERT INTO megolm_session_keys (user_id, session_id, encrypted_key, created_ts, expires_at)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT (user_id, session_id) DO UPDATE
                SET encrypted_key = EXCLUDED.encrypted_key,
                    created_ts = EXCLUDED.created_ts,
                    expires_at = EXCLUDED.expires_at
                ",
                user_id,
                session_id,
                encrypted_key,
                created_ts,
                expires_at,
            )
            .execute(&*self.pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to upsert megolm session key: {e}");
                ApiError::database("A database error occurred".to_string())
            })?;
            total += result.rows_affected();
        }
        Ok(total)
    }

    /// 单用户查询共享的 session key（vodozemac import_session 后使用）
    pub async fn get_session_key(&self, user_id: &str, session_id: &str) -> Result<Option<String>, ApiError> {
        let row: Option<MegolmSessionKeyRow> = sqlx::query_as!(
            MegolmSessionKeyRow,
            r#"
            SELECT encrypted_key AS "encrypted_key!"
            FROM megolm_session_keys
            WHERE user_id = $1 AND session_id = $2
            "#,
            user_id,
            session_id,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load megolm session key: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(row.map(|r| r.encrypted_key))
    }

    // ========================================================================
    // Phase 2 (Megolm 双写): 懒迁移辅助方法
    // ========================================================================

    /// 将已有 legacy session 升级为 dual 格式（追加 vodozemac_pickle）
    ///
    /// 仅在 `pickle_format = 'legacy'` 时执行，避免重复写入。
    /// 返回是否实际更新了行。
    pub async fn promote_to_dual(
        &self,
        session_id: &str,
        vodozemac_pickle: &str,
        now_ms: i64,
    ) -> Result<bool, ApiError> {
        let result = sqlx::query!(
            r"
            UPDATE megolm_sessions
            SET pickle_format = 'dual',
                vodozemac_pickle = $2,
                last_used_ts = $3
            WHERE session_id = $1
              AND pickle_format = 'legacy'
              AND vodozemac_pickle IS NULL
            ",
            session_id,
            vodozemac_pickle,
            now_ms,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to promote megolm session to dual: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(result.rows_affected() > 0)
    }

    /// 分页查询存量 legacy session（懒迁移扫描）
    ///
    /// 按 `session_id` 排序确保多次调用结果稳定；游标分页避免内存爆炸。
    pub async fn list_legacy_sessions(
        &self,
        after_session_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<MegolmSession>, ApiError> {
        let limit = limit.clamp(1, 1000);
        let rows: Vec<MegolmSessionRow> = match after_session_id {
            Some(cursor) => sqlx::query_as!(
                MegolmSessionRow,
                r#"
                SELECT
                    id AS "id!",
                    session_id AS "session_id!",
                    room_id AS "room_id!",
                    sender_key AS "sender_key!",
                    session_key AS "session_key!",
                    algorithm AS "algorithm!",
                    message_index AS "message_index!",
                    created_ts AS "created_ts!",
                    last_used_ts AS "last_used_ts?",
                    expires_at AS "expires_at?",
                    pickle_format AS "pickle_format!",
                    vodozemac_pickle AS "vodozemac_pickle?"
                FROM megolm_sessions
                WHERE pickle_format = 'legacy'
                  AND session_id > $1
                ORDER BY session_id ASC
                LIMIT $2
                "#,
                cursor,
                limit,
            )
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to list legacy megolm sessions: {e}");
                ApiError::database("A database error occurred".to_string())
            })?,
            None => sqlx::query_as!(
                MegolmSessionRow,
                r#"
                SELECT
                    id AS "id!",
                    session_id AS "session_id!",
                    room_id AS "room_id!",
                    sender_key AS "sender_key!",
                    session_key AS "session_key!",
                    algorithm AS "algorithm!",
                    message_index AS "message_index!",
                    created_ts AS "created_ts!",
                    last_used_ts AS "last_used_ts?",
                    expires_at AS "expires_at?",
                    pickle_format AS "pickle_format!",
                    vodozemac_pickle AS "vodozemac_pickle?"
                FROM megolm_sessions
                WHERE pickle_format = 'legacy'
                ORDER BY session_id ASC
                LIMIT $1
                "#,
                limit,
            )
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to list legacy megolm sessions: {e}");
                ApiError::database("A database error occurred".to_string())
            })?,
        };

        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// 统计各 pickle_format 的 session 数量（监控/迁移进度）
    pub async fn count_by_pickle_format(&self) -> Result<Vec<(String, i64)>, ApiError> {
        let rows = sqlx::query!(
            r#"
            SELECT pickle_format AS "pickle_format!", COUNT(*) AS "cnt!"
            FROM megolm_sessions
            GROUP BY pickle_format
            "#,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to count megolm sessions by pickle format: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(rows.into_iter().map(|r| (r.pickle_format, r.cnt)).collect())
    }

    /// Clean up expired Megolm sessions.
    ///
    /// Aligned with Synapse v1.153 behavior: sessions with a non-null `expires_at`
    /// that is past the current time are removed from the database.
    ///
    /// Returns the number of sessions deleted.
    pub async fn cleanup_expired_sessions(&self) -> Result<u64, ApiError> {
        let now_ms = Utc::now().timestamp_millis();

        let result = sqlx::query!(
            r"
            DELETE FROM megolm_sessions
            WHERE expires_at IS NOT NULL
              AND expires_at < $1
            ",
            now_ms,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to cleanup expired megolm sessions: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(result.rows_affected())
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct MegolmIncrementRow {
    message_index: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct MegolmSessionKeyRow {
    encrypted_key: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn create_test_session() -> MegolmSession {
        MegolmSession {
            id: uuid::Uuid::new_v4(),
            session_id: format!("test_session_{}", uuid::Uuid::new_v4()),
            room_id: "!testroom:example.com".to_string(),
            sender_key: "test_sender_key_base64".to_string(),
            session_key: "test_session_key_base64".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            message_index: 0,
            created_ts: Utc::now(),
            last_used_ts: Utc::now(),
            expires_at: None,
            pickle_format: PickleFormat::Legacy,
            vodozemac_pickle: None,
        }
    }

    fn create_dual_test_session() -> MegolmSession {
        let mut s = create_test_session();
        s.pickle_format = PickleFormat::Dual;
        s.vodozemac_pickle = Some("base64_vodozemac_pickle".to_string());
        s
    }

    #[test]
    fn test_megolm_session_storage_creation() {
        let session = create_test_session();

        assert!(!session.session_id.is_empty());
        assert!(!session.room_id.is_empty());
        assert!(!session.sender_key.is_empty());
        assert!(!session.session_key.is_empty());
        assert_eq!(session.algorithm, "m.megolm.v1.aes-sha2");
        assert_eq!(session.pickle_format, PickleFormat::Legacy);
        assert!(session.vodozemac_pickle.is_none());
    }

    #[test]
    fn test_megolm_session_dual_format() {
        let session = create_dual_test_session();
        assert_eq!(session.pickle_format, PickleFormat::Dual);
        assert!(session.vodozemac_pickle.is_some());
        assert_eq!(session.vodozemac_pickle.as_deref(), Some("base64_vodozemac_pickle"));
    }

    #[test]
    fn test_megolm_session_field_validation() {
        let session = create_test_session();

        assert!(session.room_id.starts_with('!'), "Room ID should start with !");
        assert!(session.algorithm.starts_with("m.megolm"), "Algorithm should be megolm");
        assert!(session.message_index >= 0, "Message index should be non-negative");
    }

    #[test]
    fn test_megolm_session_with_expiry() {
        let expiry_time = Utc::now() + Duration::hours(24);
        let mut session = create_test_session();
        session.expires_at = Some(expiry_time);

        assert!(session.expires_at.is_some());
        let expires = session.expires_at.unwrap();
        assert!(expires > Utc::now(), "Expiry time should be in the future");
        assert!(expires > session.created_ts, "Expiry should be after creation");
    }

    #[test]
    fn test_megolm_session_without_expiry() {
        let session = create_test_session();

        assert!(session.expires_at.is_none(), "Session should not have expiry by default");
    }

    #[test]
    fn test_megolm_session_message_index_increment() {
        let mut session = create_test_session();

        assert_eq!(session.message_index, 0);

        session.message_index += 1;
        assert_eq!(session.message_index, 1);

        session.message_index = 100;
        assert_eq!(session.message_index, 100);
    }

    #[test]
    fn test_megolm_session_last_used_update() {
        let mut session = create_test_session();
        let original_last_used = session.last_used_ts;

        std::thread::sleep(std::time::Duration::from_millis(10));
        session.last_used_ts = Utc::now();

        assert!(session.last_used_ts > original_last_used, "Last used should be updated");
    }

    #[test]
    fn test_megolm_session_algorithm_validation() {
        let valid_algorithms = vec!["m.megolm.v1.aes-sha2"];

        for algo in valid_algorithms {
            let mut session = create_test_session();
            session.algorithm = algo.to_string();

            assert!(session.algorithm.starts_with("m.megolm"));
            assert!(session.algorithm.contains("aes-sha2"));
        }
    }

    #[test]
    fn test_megolm_session_room_id_format() {
        let session = create_test_session();

        assert!(session.room_id.starts_with('!'), "Room ID must start with !");
        assert!(session.room_id.contains(':'), "Room ID must contain ':' separator");

        let parts: Vec<&str> = session.room_id[1..].split(':').collect();
        assert!(parts.len() >= 2, "Room ID should have localpart and server name");
    }

    #[test]
    fn test_megolm_session_key_base64_format() {
        let session = create_test_session();

        assert!(!session.session_key.is_empty(), "Session key should not be empty");
        assert!(!session.sender_key.is_empty(), "Sender key should not be empty");

        assert!(session.session_key.len() > 10, "Session key should have reasonable length");
        assert!(session.sender_key.len() > 10, "Sender key should have reasonable length");
    }

    #[test]
    fn test_megolm_session_boundary_conditions() {
        let mut session = create_test_session();

        session.message_index = i64::MAX;
        assert_eq!(session.message_index, i64::MAX);

        session.message_index = 0;
        assert_eq!(session.message_index, 0);
    }

    #[test]
    fn test_megolm_session_time_ordering() {
        let created = Utc::now() - Duration::hours(1);
        let last_used = Utc::now();
        let expires = Utc::now() + Duration::hours(24);

        let session = MegolmSession {
            id: uuid::Uuid::new_v4(),
            session_id: "time_test".to_string(),
            room_id: "!room:example.com".to_string(),
            sender_key: "key".to_string(),
            session_key: "key".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            message_index: 0,
            created_ts: created,
            last_used_ts: last_used,
            expires_at: Some(expires),
            pickle_format: PickleFormat::Legacy,
            vodozemac_pickle: None,
        };

        assert!(session.created_ts <= session.last_used_ts);
        assert!(session.last_used_ts <= session.expires_at.unwrap());
    }

    #[test]
    fn test_megolm_session_id_uniqueness() {
        let session1 = create_test_session();
        let session2 = create_test_session();

        assert_ne!(session1.id, session2.id, "Session IDs should be unique");
        assert_ne!(session1.session_id, session2.session_id, "Session identifiers should be unique");
    }

    #[test]
    fn test_megolm_session_clone() {
        let session = create_test_session();
        let cloned = session.clone();

        assert_eq!(session.id, cloned.id);
        assert_eq!(session.session_id, cloned.session_id);
        assert_eq!(session.room_id, cloned.room_id);
        assert_eq!(session.algorithm, cloned.algorithm);
        assert_eq!(session.message_index, cloned.message_index);
    }
}

use super::models::*;
use crate::crypto::key_at_rest::{KeyAtRest, AT_REST_VERSION_PREFIX};
use chrono::Utc;
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::map_database;
use synapse_common::ApiError;

/// Internal row struct for `megolm_sessions` (matches DB column types exactly,
/// including BIGINT timestamps that the public model converts to DateTime<Utc>).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MegolmSessionRow {
    /// The `id` field.
    pub id: uuid::Uuid,
    /// The `session_id` field.
    pub session_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `sender_key` field.
    pub sender_key: String,
    /// The `session_key` field.
    pub session_key: String,
    /// The `algorithm` field.
    pub algorithm: String,
    /// The `message_index` field.
    pub message_index: i64,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `last_used_ts` field.
    pub last_used_ts: Option<i64>,
    /// The `expires_at` field.
    pub expires_at: Option<i64>,
}

/// Implementation of [`From`] methods.
impl From<MegolmSessionRow> for MegolmSession {
    fn from(row: MegolmSessionRow) -> Self {
        let created_ts_dt = chrono::DateTime::from_timestamp_millis(row.created_ts).unwrap_or_else(|| {
            tracing::warn!("Invalid created_ts {} for session {}, using current time", row.created_ts, row.session_id);
            Utc::now()
        });
        let last_used_ts_dt = row.last_used_ts.and_then(chrono::DateTime::from_timestamp_millis).unwrap_or_else(|| {
            tracing::warn!(
                "Invalid last_used_ts {} for session {}, using created_ts",
                row.last_used_ts.unwrap_or(0),
                row.session_id
            );
            created_ts_dt
        });
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
        }
    }
}

#[derive(Clone)]
/// The `MegolmSessionStorage` type.
pub struct MegolmSessionStorage {
    /// The `pool` field.
    pub pool: Arc<PgPool>,
    /// At-rest key used to `seal`/`open` the `session_key` column (S-10).
    pub at_rest: KeyAtRest,
}

/// Implementation of [`MegolmSessionStorage`] methods.
impl MegolmSessionStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<PgPool>, at_rest: KeyAtRest) -> Self {
        Self { pool: pool.clone(), at_rest }
    }

    /// Encrypt a `session_key` for storage (S-10): AES-256-GCM, `v1:`-prefixed.
    fn seal_session_key(&self, plaintext: &str) -> Result<String, ApiError> {
        self.at_rest.seal(plaintext.as_bytes())
    }

    /// Decrypt a stored `session_key`.
    ///
    /// Rows written before at-rest encryption (S-10) hold a legacy plaintext
    /// pickle with no `v1:` prefix; they are returned verbatim and re-sealed by
    /// the next write, which is the lazy migration path.
    fn open_session_key(&self, stored: &str) -> Result<String, ApiError> {
        if stored.starts_with(AT_REST_VERSION_PREFIX) {
            let plaintext = self.at_rest.open(stored)?;
            String::from_utf8(plaintext).map_err(|_| ApiError::internal("megolm session key is not valid UTF-8"))
        } else {
            Ok(stored.to_string())
        }
    }

    /// See [`create_session`].
    pub async fn create_session(&self, session: &MegolmSession) -> Result<(), ApiError> {
        let sealed_key = self.seal_session_key(&session.session_key)?;
        sqlx::query!(
            r#"
            INSERT INTO megolm_sessions (
                id, session_id, room_id, sender_key, session_key, algorithm,
                message_index, created_ts, last_used_ts, expires_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
            session.id,
            &session.session_id,
            &session.room_id,
            &session.sender_key,
            &sealed_key,
            &session.algorithm,
            session.message_index,
            session.created_ts.timestamp_millis(),
            session.last_used_ts.timestamp_millis(),
            session.expires_at.map(|t| t.timestamp_millis()),
        )
        .execute(&*self.pool)
        .await
        .map_err(map_database!("Failed to create megolm session"))?;

        Ok(())
    }

    /// See [`get_session`].
    pub async fn get_session(&self, session_id: &str) -> Result<Option<MegolmSession>, ApiError> {
        let row = sqlx::query_as!(
            MegolmSessionRow,
            r#"
            SELECT
                id,
                session_id,
                room_id,
                sender_key,
                session_key,
                algorithm,
                message_index,
                created_ts,
                last_used_ts,
                expires_at
            FROM megolm_sessions
            WHERE session_id = $1
            "#,
            session_id,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(map_database!("Failed to load megolm session"))?;

        match row {
            Some(row) => {
                let mut session: MegolmSession = row.into();
                session.session_key = self.open_session_key(&session.session_key)?;
                Ok(Some(session))
            }
            None => Ok(None),
        }
    }

    /// See [`get_room_sessions`].
    pub async fn get_room_sessions(&self, room_id: &str) -> Result<Vec<MegolmSession>, ApiError> {
        let rows = sqlx::query_as!(
            MegolmSessionRow,
            r#"
            SELECT
                id,
                session_id,
                room_id,
                sender_key,
                session_key,
                algorithm,
                message_index,
                created_ts,
                last_used_ts,
                expires_at
            FROM megolm_sessions
            WHERE room_id = $1
            "#,
            room_id,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(map_database!("Failed to load megolm sessions"))?;

        let mut sessions: Vec<MegolmSession> = rows.into_iter().map(Into::into).collect();
        for session in &mut sessions {
            session.session_key = self.open_session_key(&session.session_key)?;
        }
        Ok(sessions)
    }

    /// See [`update_session`].
    pub async fn update_session(&self, session: &MegolmSession) -> Result<(), ApiError> {
        let sealed_key = self.seal_session_key(&session.session_key)?;
        sqlx::query!(
            r#"
            UPDATE megolm_sessions
            SET session_key = $2,
                message_index = $3,
                last_used_ts = $4,
                expires_at = $5
            WHERE session_id = $1
            "#,
            &session.session_id,
            &sealed_key,
            session.message_index,
            session.last_used_ts.timestamp_millis(),
            session.expires_at.map(|t| t.timestamp_millis()),
        )
        .execute(&*self.pool)
        .await
        .map_err(map_database!("Failed to update megolm session"))?;
        Ok(())
    }

    /// See [`delete_session`].
    pub async fn delete_session(&self, session_id: &str) -> Result<(), ApiError> {
        sqlx::query!(
            r#"
            DELETE FROM megolm_sessions
            WHERE session_id = $1
            "#,
            session_id,
        )
        .execute(&*self.pool)
        .await
        .map_err(map_database!("Failed to delete megolm session"))?;

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
        let row = sqlx::query_as!(
            MegolmIncrementRow,
            r#"
            UPDATE megolm_sessions
            SET message_index = message_index + $2,
                last_used_ts = $3
            WHERE session_id = $1
            RETURNING message_index
            "#,
            session_id,
            delta,
            now_ms,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(map_database!("Failed to increment megolm message index"))?;

        Ok(row.map(|r| r.message_index))
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

        let result = sqlx::query!(
            r#"
            INSERT INTO megolm_session_keys (user_id, session_id, encrypted_key, created_ts, expires_at)
            SELECT unnest($1::text[]), $2, $3, $4, $5
            ON CONFLICT (user_id, session_id) DO UPDATE
            SET encrypted_key = EXCLUDED.encrypted_key,
                created_ts = EXCLUDED.created_ts,
                expires_at = EXCLUDED.expires_at
            "#,
            user_ids,
            session_id,
            encrypted_key,
            created_ts,
            expires_at,
        )
        .execute(&*self.pool)
        .await
        .map_err(map_database!("Failed to batch upsert megolm session keys"))?;

        Ok(result.rows_affected())
    }

    /// 单用户查询共享的 session key（接收方读取共享密钥时使用）
    pub async fn get_session_key(&self, user_id: &str, session_id: &str) -> Result<Option<String>, ApiError> {
        let row = sqlx::query_as!(
            MegolmSessionKeyRow,
            r#"
            SELECT encrypted_key
            FROM megolm_session_keys
            WHERE user_id = $1 AND session_id = $2
            "#,
            user_id,
            session_id,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(map_database!("Failed to load megolm session key"))?;

        Ok(row.map(|r| r.encrypted_key))
    }

    /// Clean up expired Megolm sessions.
    ///
    /// Aligned with Synapse v1.153 behavior: sessions with a non-null `expires_at`
    /// that is past the current time are removed from the database.
    ///
    /// Returns the number of sessions deleted.
    pub async fn cleanup_expired_sessions(&self) -> Result<u64, ApiError> {
        let now_ms = current_timestamp_millis();

        let result = sqlx::query!(
            r#"
            DELETE FROM megolm_sessions
            WHERE expires_at IS NOT NULL
              AND expires_at < $1
            "#,
            now_ms,
        )
        .execute(&*self.pool)
        .await
        .map_err(map_database!("Failed to cleanup expired megolm sessions"))?;

        Ok(result.rows_affected())
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
/// The `MegolmIncrementRow` type.
struct MegolmIncrementRow {
    /// The `message_index` field.
    message_index: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
/// The `MegolmSessionKeyRow` type.
struct MegolmSessionKeyRow {
    /// The `encrypted_key` field.
    encrypted_key: String,
}

#[cfg(test)]
mod db_tests;
#[cfg(test)]
mod tests;

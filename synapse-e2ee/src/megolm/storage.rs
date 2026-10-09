use super::models::*;
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
}

/// Implementation of [`MegolmSessionStorage`] methods.
impl MegolmSessionStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// See [`create_session`].
    pub async fn create_session(&self, session: &MegolmSession) -> Result<(), ApiError> {
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
            &session.session_key,
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

        Ok(row.map(Into::into))
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

        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// See [`update_session`].
    pub async fn update_session(&self, session: &MegolmSession) -> Result<(), ApiError> {
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
            &session.session_key,
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
        }
    }

    #[test]
    fn test_megolm_session_storage_creation() {
        let session = MegolmSession {
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
        };

        assert!(!session.session_id.is_empty());
        assert!(!session.room_id.is_empty());
        assert!(!session.sender_key.is_empty());
        assert!(!session.session_key.is_empty());
        assert_eq!(session.algorithm, "m.megolm.v1.aes-sha2");
    }

    #[test]
    fn test_megolm_session_field_validation() {
        let session = MegolmSession {
            id: uuid::Uuid::new_v4(),
            session_id: "test".to_string(),
            room_id: "!testroom:example.com".to_string(),
            sender_key: "test_sender_key_base64".to_string(),
            session_key: "test_session_key_base64".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            message_index: 0,
            created_ts: Utc::now(),
            last_used_ts: Utc::now(),
            expires_at: None,
        };

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

/// DB round trip for the 10 statements converted in C26, run against the real v12
/// baseline.
///
/// Before C26 this module had **no DB coverage at all**: every case in the module
/// above is a pure constructor/serialization check. That is the blind spot which
/// hid D-46/D-49 behind simplified fixtures (D-36/D-47 family), and this file is
/// where D-49's second site lives (`megolm_sessions.message_index`, tightened to
/// `NOT NULL DEFAULT 0` in the C26 "fix first" commit — which is why the
/// `MegolmSessionRow.message_index: i64` projection needs no `AS "col!"`).
///
/// `megolm_sessions` / `megolm_session_keys` carry no foreign keys in the baseline,
/// so no seed rows are needed (contrast `backup::storage::db_tests`, which must
/// create the `rooms` row `fk_backup_keys_room` demands).
#[cfg(test)]
mod db_tests {
    use super::*;
    use synapse_common::test_isolation::IsolatedTestPool;

    /// The workspace baseline migration. The bytes are load-bearing (the shared
    /// template name is a content fingerprint of this string), so it must stay
    /// byte-identical to the copies in `synapse-storage/src/test_isolation.rs`,
    /// `synapse-e2ee/src/olm/storage.rs` and `synapse-services/src/test_utils.rs`.
    const BASELINE_SQL: &str = include_str!("../../../migrations/00000000_unified_schema_v12.sql");

    fn make_session(session_id: &str, room_id: &str, index: i64) -> MegolmSession {
        let created = Utc::now();
        MegolmSession {
            id: uuid::Uuid::new_v4(),
            session_id: session_id.to_string(),
            room_id: room_id.to_string(),
            sender_key: format!("sender-{session_id}"),
            session_key: format!("pickle-{session_id}"),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            message_index: index,
            created_ts: created,
            last_used_ts: created,
            expires_at: None,
        }
    }

    #[tokio::test]
    async fn test_megolm_round_trip_on_migration_template() {
        let isolated = IsolatedTestPool::new(BASELINE_SQL).await.expect("isolated test pool");
        let pool = isolated.pool();
        let storage = MegolmSessionStorage::new(&pool);

        let room_a = "!c26a:localhost";
        let room_b = "!c26b:localhost";

        // --- create_session -> get_session ---
        let s1 = make_session("sess-a1", room_a, 0);
        storage.create_session(&s1).await.unwrap();
        storage.create_session(&make_session("sess-a2", room_a, 7)).await.unwrap();
        storage.create_session(&make_session("sess-b1", room_b, 0)).await.unwrap();

        let loaded = storage.get_session("sess-a1").await.unwrap().expect("sess-a1");
        assert_eq!(loaded.id, s1.id);
        assert_eq!(loaded.session_id, "sess-a1");
        assert_eq!(loaded.room_id, room_a);
        assert_eq!(loaded.sender_key, "sender-sess-a1");
        assert_eq!(loaded.session_key, "pickle-sess-a1");
        assert_eq!(loaded.algorithm, "m.megolm.v1.aes-sha2");
        assert_eq!(loaded.message_index, 0);
        assert_eq!(loaded.created_ts.timestamp_millis(), s1.created_ts.timestamp_millis());
        assert_eq!(loaded.last_used_ts.timestamp_millis(), s1.last_used_ts.timestamp_millis());
        assert_eq!(loaded.expires_at, None);
        assert!(storage.get_session("missing").await.unwrap().is_none());

        // `session_id` is UNIQUE and `create_session` has no `ON CONFLICT`: the second
        // insert must surface a mapped DB error, not silently become an update.
        let duplicate = storage.create_session(&make_session("sess-a1", room_a, 0)).await;
        let err = duplicate.expect_err("a duplicate session_id must hit the UNIQUE constraint");
        assert_eq!(err.message, "Database error: Failed to create megolm session");

        // --- get_room_sessions ---
        // `message_index = 7` must survive as `i64` (D-49's second site) and the
        // projection must not need an `AS "message_index!"` assertion.
        let mut in_a = storage.get_room_sessions(room_a).await.unwrap();
        in_a.sort_by_key(|s| s.message_index);
        assert_eq!(in_a.iter().map(|s| s.session_id.as_str()).collect::<Vec<_>>(), vec!["sess-a1", "sess-a2"]);
        assert_eq!(in_a[1].message_index, 7);
        assert_eq!(storage.get_room_sessions(room_b).await.unwrap().len(), 1);
        assert!(storage.get_room_sessions("!empty:localhost").await.unwrap().is_empty());

        // --- update_session ---
        // Only the five SET columns may change; `created_ts`/`room_id` must survive.
        let mut updated = s1.clone();
        updated.session_key = "pickle-rotated".to_string();
        updated.message_index = 42;
        updated.last_used_ts = s1.last_used_ts + chrono::Duration::seconds(60);
        updated.expires_at = Some(s1.created_ts + chrono::Duration::hours(1));
        storage.update_session(&updated).await.unwrap();

        let reloaded = storage.get_session("sess-a1").await.unwrap().expect("sess-a1 after update");
        assert_eq!(reloaded.session_key, "pickle-rotated");
        assert_eq!(reloaded.message_index, 42);
        assert_eq!(reloaded.last_used_ts.timestamp_millis(), updated.last_used_ts.timestamp_millis());
        assert_eq!(reloaded.expires_at.map(|t| t.timestamp_millis()), updated.expires_at.map(|t| t.timestamp_millis()));
        assert_eq!(reloaded.room_id, room_a, "update_session must not touch room_id");
        assert_eq!(
            reloaded.created_ts.timestamp_millis(),
            s1.created_ts.timestamp_millis(),
            "update_session must not touch created_ts"
        );

        // --- increment_message_index (atomic UPDATE ... RETURNING) ---
        assert_eq!(storage.increment_message_index("sess-a1", 3, 1_000).await.unwrap(), Some(45));
        assert_eq!(storage.increment_message_index("sess-a1", 0, 2_000).await.unwrap(), Some(45));
        assert_eq!(
            storage.increment_message_index("missing", 1, 0).await.unwrap(),
            None,
            "an unknown session must yield None, not a fabricated index"
        );
        let after_increment = storage.get_session("sess-a1").await.unwrap().unwrap();
        assert_eq!(after_increment.message_index, 45);
        // The same statement also writes `last_used_ts`.
        assert_eq!(after_increment.last_used_ts.timestamp_millis(), 2_000);

        // --- cleanup_expired_sessions: only `expires_at IS NOT NULL AND < now` ---
        let mut past = make_session("sess-past", room_a, 0);
        past.expires_at = Some(Utc::now() - chrono::Duration::hours(1));
        storage.create_session(&past).await.unwrap();
        let mut future = make_session("sess-future", room_a, 0);
        future.expires_at = Some(Utc::now() + chrono::Duration::hours(1));
        storage.create_session(&future).await.unwrap();

        assert_eq!(storage.cleanup_expired_sessions().await.unwrap(), 1, "only the past-expiry row is due");
        assert!(storage.get_session("sess-past").await.unwrap().is_none());
        // A `NULL` expiry (sess-a2, sess-b1) and a non-NULL but not-yet-due expiry
        // (sess-future) must both survive.
        assert!(storage.get_session("sess-future").await.unwrap().is_some());
        assert!(storage.get_session("sess-a2").await.unwrap().is_some());
        assert!(storage.get_session("sess-b1").await.unwrap().is_some());

        // --- upsert_session_keys_batch / get_session_key ---
        assert_eq!(
            storage.upsert_session_keys_batch(&[], "sess-a1", "k", 1, None).await.unwrap(),
            0,
            "an empty user list must short-circuit to 0 without touching the table"
        );

        let users = vec!["@c26a:localhost".to_string(), "@c26b:localhost".to_string()];
        assert_eq!(storage.upsert_session_keys_batch(&users, "sess-a1", "enc-1", 100, None).await.unwrap(), 2);
        assert_eq!(storage.get_session_key("@c26a:localhost", "sess-a1").await.unwrap().as_deref(), Some("enc-1"));
        assert!(storage.get_session_key("@c26a:localhost", "sess-missing").await.unwrap().is_none());

        // Re-upserting the same (user_id, session_id) pairs must update in place, not
        // duplicate: `rows_affected` stays 2 and the key is rewritten.
        assert_eq!(
            storage.upsert_session_keys_batch(&users, "sess-a1", "enc-2", 200, Some(9_000_000_000_000)).await.unwrap(),
            2
        );
        assert_eq!(storage.get_session_key("@c26b:localhost", "sess-a1").await.unwrap().as_deref(), Some("enc-2"));
        let key_rows: i64 =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM megolm_session_keys").fetch_one(&*pool).await.unwrap();
        assert_eq!(key_rows, 2, "the ON CONFLICT upsert must not duplicate rows");
        // A second session for the same users must add exactly two more rows.
        assert_eq!(storage.upsert_session_keys_batch(&users, "sess-b1", "enc-3", 300, None).await.unwrap(), 2);
        let key_rows: i64 =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM megolm_session_keys").fetch_one(&*pool).await.unwrap();
        assert_eq!(key_rows, 4, "(user_id, session_id) is the conflict target, not user_id alone");

        // --- delete_session ---
        storage.delete_session("sess-a1").await.unwrap();
        assert!(storage.get_session("sess-a1").await.unwrap().is_none());
        // Deleting an already-absent row is a no-op, not an error.
        storage.delete_session("sess-a1").await.unwrap();
        // Deleting a session does not cascade to `megolm_session_keys` (no FK):
        // the keys are cleaned up by their own path. Pin that so an added FK is noticed.
        let key_rows: i64 =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM megolm_session_keys WHERE session_id = 'sess-a1'")
                .fetch_one(&*pool)
                .await
                .unwrap();
        assert_eq!(key_rows, 2, "megolm_session_keys has no FK to megolm_sessions in the baseline");
    }
}

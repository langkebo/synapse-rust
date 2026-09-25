use super::models::{OlmAccountData, OlmSessionData};
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::map_database;
use synapse_common::ApiError;

/// Internal row struct for `olm_sessions` (matches DB column types exactly,
/// including `i32` for `message_index` which the public model widens to `u32`).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OlmSessionRow {
    /// The `session_id` field.
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `sender_key` field.
    /// The `receiver_key` field.
    /// The `serialized_state` field.
    /// The `message_index` field.
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub session_id: String,
    /// The `user_id` field.
    /// The `device_id` field.
    /// The `sender_key` field.
    /// The `receiver_key` field.
    /// The `serialized_state` field.
    /// The `message_index` field.
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub user_id: String,
    /// The `device_id` field.
    /// The `sender_key` field.
    /// The `receiver_key` field.
    /// The `serialized_state` field.
    /// The `message_index` field.
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub device_id: String,
    /// The `sender_key` field.
    /// The `receiver_key` field.
    /// The `serialized_state` field.
    /// The `message_index` field.
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub sender_key: String,
    /// The `receiver_key` field.
    /// The `serialized_state` field.
    /// The `message_index` field.
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub receiver_key: String,
    /// The `serialized_state` field.
    /// The `message_index` field.
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub serialized_state: String,
    /// The `message_index` field.
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub message_index: i32,
    /// The `created_ts` field.
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub created_ts: i64,
    /// The `last_used_ts` field.
    /// The `expires_at` field.
    pub last_used_ts: i64,
    /// The `expires_at` field.
    pub expires_at: Option<i64>,
}

/// Implementation of [`From`] methods.
impl From<OlmSessionRow> for OlmSessionData {
    fn from(row: OlmSessionRow) -> Self {
        OlmSessionData {
            session_id: row.session_id,
            user_id: row.user_id,
            device_id: row.device_id,
            sender_key: row.sender_key,
            receiver_key: row.receiver_key,
            serialized_state: row.serialized_state,
            message_index: row.message_index as u32,
            created_ts: row.created_ts,
            last_used_ts: row.last_used_ts,
            expires_at: row.expires_at,
        }
    }
}

#[derive(Clone)]
/// The `OlmStorage` type.
pub struct OlmStorage {
    pool: Arc<PgPool>,
}

/// Implementation of [`OlmStorage`] methods.
impl OlmStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// See [`save_account`].
    pub async fn save_account(&self, account: &OlmAccountData) -> Result<(), ApiError> {
        let now = current_timestamp_millis();

        sqlx::query!(
            r#"
            INSERT INTO olm_accounts (
                user_id, device_id, identity_key, serialized_account,
                is_one_time_keys_published, is_fallback_key_published, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (user_id, device_id) DO UPDATE SET
                identity_key = EXCLUDED.identity_key,
                serialized_account = EXCLUDED.serialized_account,
                is_one_time_keys_published = EXCLUDED.is_one_time_keys_published,
                is_fallback_key_published = EXCLUDED.is_fallback_key_published,
                updated_ts = EXCLUDED.updated_ts
            "#,
            &account.user_id,
            &account.device_id,
            &account.identity_key,
            &account.serialized_account,
            account.has_published_one_time_keys,
            account.has_published_fallback_key,
            now,
            now,
        )
        .execute(&*self.pool)
        .await
        .map_err(map_database!("Failed to save olm account"))?;

        Ok(())
    }

    /// See [`load_account`].
    pub async fn load_account(&self, user_id: &str, device_id: &str) -> Result<Option<OlmAccountData>, ApiError> {
        let row = sqlx::query_as!(
            OlmAccountRow,
            r#"
            SELECT
                user_id,
                device_id,
                identity_key,
                serialized_account,
                is_one_time_keys_published,
                is_fallback_key_published
            FROM olm_accounts
            WHERE user_id = $1 AND device_id = $2
            "#,
            user_id,
            device_id,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(map_database!("Failed to load olm account"))?;

        Ok(row.map(|r| OlmAccountData {
            user_id: r.user_id,
            device_id: r.device_id,
            identity_key: r.identity_key,
            serialized_account: r.serialized_account,
            has_published_one_time_keys: r.is_one_time_keys_published.unwrap_or(false),
            has_published_fallback_key: r.is_fallback_key_published.unwrap_or(false),
        }))
    }

    /// See [`delete_account`].
    pub async fn delete_account(&self, user_id: &str, device_id: &str) -> Result<(), ApiError> {
        sqlx::query!(
            r#"
            DELETE FROM olm_accounts
            WHERE user_id = $1 AND device_id = $2
            "#,
            user_id,
            device_id,
        )
        .execute(&*self.pool)
        .await
        .map_err(map_database!("Failed to delete olm account"))?;

        self.delete_sessions_for_device(user_id, device_id).await?;

        Ok(())
    }

    /// See [`save_session`].
    pub async fn save_session(&self, session: &OlmSessionData) -> Result<(), ApiError> {
        sqlx::query!(
            r#"
            INSERT INTO olm_sessions (
                user_id, device_id, session_id, sender_key, receiver_key,
                serialized_state, message_index, created_ts, last_used_ts, expires_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (session_id) DO UPDATE SET
                serialized_state = EXCLUDED.serialized_state,
                message_index = EXCLUDED.message_index,
                last_used_ts = EXCLUDED.last_used_ts,
                expires_at = EXCLUDED.expires_at
            "#,
            &session.user_id,
            &session.device_id,
            &session.session_id,
            &session.sender_key,
            &session.receiver_key,
            &session.serialized_state,
            session.message_index as i32,
            session.created_ts,
            session.last_used_ts,
            session.expires_at,
        )
        .execute(&*self.pool)
        .await
        .map_err(map_database!("Failed to save olm session"))?;

        Ok(())
    }

    /// See [`load_sessions`].
    pub async fn load_sessions(&self, user_id: &str, device_id: &str) -> Result<Vec<OlmSessionData>, ApiError> {
        let rows = sqlx::query_as!(
            OlmSessionRow,
            r#"
            SELECT
                session_id,
                user_id,
                device_id,
                sender_key,
                receiver_key,
                serialized_state,
                message_index AS "message_index!",
                created_ts,
                last_used_ts,
                expires_at
            FROM olm_sessions
            WHERE user_id = $1 AND device_id = $2
            ORDER BY last_used_ts DESC
            "#,
            user_id,
            device_id,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(map_database!("Failed to load olm sessions"))?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// See [`load_session`].
    pub async fn load_session(&self, session_id: &str) -> Result<Option<OlmSessionData>, ApiError> {
        let row = sqlx::query_as!(
            OlmSessionRow,
            r#"
            SELECT
                session_id,
                user_id,
                device_id,
                sender_key,
                receiver_key,
                serialized_state,
                message_index AS "message_index!",
                created_ts,
                last_used_ts,
                expires_at
            FROM olm_sessions
            WHERE session_id = $1
            "#,
            session_id,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(map_database!("Failed to load olm session"))?;

        Ok(row.map(Into::into))
    }

    /// See [`load_session_by_sender_key`].
    pub async fn load_session_by_sender_key(
        &self,
        user_id: &str,
        device_id: &str,
        sender_key: &str,
    ) -> Result<Option<OlmSessionData>, ApiError> {
        let row = sqlx::query_as!(
            OlmSessionRow,
            r#"
            SELECT
                session_id,
                user_id,
                device_id,
                sender_key,
                receiver_key,
                serialized_state,
                message_index AS "message_index!",
                created_ts,
                last_used_ts,
                expires_at
            FROM olm_sessions
            WHERE user_id = $1 AND device_id = $2 AND sender_key = $3
            ORDER BY last_used_ts DESC
            LIMIT 1
            "#,
            user_id,
            device_id,
            sender_key,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(map_database!("Failed to load olm session by sender key"))?;

        Ok(row.map(Into::into))
    }

    /// See [`delete_session`].
    pub async fn delete_session(&self, session_id: &str) -> Result<(), ApiError> {
        sqlx::query!(
            r#"
            DELETE FROM olm_sessions
            WHERE session_id = $1
            "#,
            session_id,
        )
        .execute(&*self.pool)
        .await
        .map_err(map_database!("Failed to delete olm session"))?;

        Ok(())
    }

    /// See [`delete_sessions_for_device`].
    pub async fn delete_sessions_for_device(&self, user_id: &str, device_id: &str) -> Result<(), ApiError> {
        sqlx::query!(
            r#"
            DELETE FROM olm_sessions
            WHERE user_id = $1 AND device_id = $2
            "#,
            user_id,
            device_id,
        )
        .execute(&*self.pool)
        .await
        .map_err(map_database!("Failed to delete olm sessions"))?;

        Ok(())
    }

    /// See [`delete_expired_sessions`].
    pub async fn delete_expired_sessions(&self) -> Result<u64, ApiError> {
        let now = current_timestamp_millis();

        let result = sqlx::query!(
            r#"
            DELETE FROM olm_sessions
            WHERE expires_at IS NOT NULL AND expires_at < $1
            "#,
            now,
        )
        .execute(&*self.pool)
        .await
        .map_err(map_database!("Failed to delete expired sessions"))?;

        Ok(result.rows_affected())
    }

    /// See [`update_session_last_used`].
    pub async fn update_session_last_used(&self, session_id: &str) -> Result<(), ApiError> {
        let now = current_timestamp_millis();

        sqlx::query!(
            r#"
            UPDATE olm_sessions
            SET last_used_ts = $1
            WHERE session_id = $2
            "#,
            now,
            session_id,
        )
        .execute(&*self.pool)
        .await
        .map_err(map_database!("Failed to update session last used"))?;

        Ok(())
    }

    /// See [`get_session_count`].
    pub async fn get_session_count(&self, user_id: &str, device_id: &str) -> Result<i64, ApiError> {
        let count = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*)
            FROM olm_sessions
            WHERE user_id = $1 AND device_id = $2
            "#,
            user_id,
            device_id,
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(map_database!("Failed to get session count"))?
        // `COUNT(*)` 无 relation origin ⇒ 宏判可空（C19a 同型）；计数语义恒非空。
        .unwrap_or(0);

        Ok(count)
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct OlmAccountRow {
    user_id: String,
    device_id: String,
    identity_key: String,
    serialized_account: String,
    is_one_time_keys_published: Option<bool>,
    is_fallback_key_published: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_olm_account_data_serialization() {
        let account = OlmAccountData {
            user_id: "@test:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            identity_key: "test_identity_key".to_string(),
            serialized_account: "serialized_data".to_string(),
            has_published_one_time_keys: false,
            has_published_fallback_key: false,
        };

        assert_eq!(account.user_id, "@test:example.com");
        assert_eq!(account.device_id, "DEVICE123");
        assert!(!account.has_published_one_time_keys);
    }

    #[test]
    fn test_olm_session_data_serialization() {
        let session = OlmSessionData {
            session_id: "session_123".to_string(),
            user_id: "@test:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            sender_key: "sender_key".to_string(),
            receiver_key: "receiver_key".to_string(),
            serialized_state: "state_data".to_string(),
            message_index: 5,
            created_ts: 1234567890,
            last_used_ts: 1234567900,
            expires_at: Some(1234568000),
        };

        assert_eq!(session.session_id, "session_123");
        assert_eq!(session.message_index, 5);
        assert!(session.expires_at.is_some());
    }

    #[test]
    fn test_olm_session_row_to_data_widens_message_index() {
        // OlmSessionRow.message_index 是 i32（DB 列类型），转换为 OlmSessionData 时
        // 扩宽为 u32。负数会按 as 转换（bit 拷贝），这里只验证正常非负路径。
        let row = OlmSessionRow {
            session_id: "s1".to_string(),
            user_id: "@u:example.com".to_string(),
            device_id: "D1".to_string(),
            sender_key: "sk".to_string(),
            receiver_key: "rk".to_string(),
            serialized_state: "state".to_string(),
            message_index: 42,
            created_ts: 100,
            last_used_ts: 200,
            expires_at: Some(300),
        };
        let data: OlmSessionData = row.into();
        assert_eq!(data.session_id, "s1");
        assert_eq!(data.message_index, 42u32);
        assert_eq!(data.created_ts, 100);
        assert_eq!(data.last_used_ts, 200);
        assert_eq!(data.expires_at, Some(300));
    }

    #[test]
    fn test_olm_session_row_to_data_none_expiry() {
        let row = OlmSessionRow {
            session_id: "s2".to_string(),
            user_id: "@u:example.com".to_string(),
            device_id: "D2".to_string(),
            sender_key: "sk".to_string(),
            receiver_key: "rk".to_string(),
            serialized_state: "state".to_string(),
            message_index: 0,
            created_ts: 100,
            last_used_ts: 200,
            expires_at: None,
        };
        let data: OlmSessionData = row.into();
        assert_eq!(data.message_index, 0u32);
        assert!(data.expires_at.is_none());
    }
}

/// DB round trip for the 12 statements converted in C25, run against the real v12
/// baseline.
///
/// Before C25 this module had **no DB coverage at all**: every case in the module
/// above is a pure constructor/serialization check, so nothing ever compared
/// `OlmAccountRow` / `OlmSessionRow` against the real catalog. That is the same
/// blind spot that let D-46 hide behind a hand-built fixture (D-36/D-47 family).
/// `IsolatedTestPool` compiles the workspace baseline in, so a
/// schema/type/nullability mismatch surfaces on the first round trip.
///
/// `olm_accounts` / `olm_sessions` carry no foreign keys in the baseline, so no
/// seed rows are needed (contrast `backup::storage::db_tests`, which must create
/// the `rooms` row required by `fk_backup_keys_room`).
#[cfg(test)]
mod db_tests {
    use super::*;
    use synapse_common::test_isolation::IsolatedTestPool;

    /// The workspace baseline migration, compiled in so the isolated schema is
    /// the real one. The bytes are load-bearing (the shared template name is a
    /// content fingerprint of this string), so it must stay byte-identical to the
    /// copies in `synapse-storage/src/test_isolation.rs`,
    /// `synapse-e2ee/src/backup/storage.rs` and
    /// `synapse-services/src/test_utils.rs`.
    const BASELINE_SQL: &str = include_str!("../../../migrations/00000000_unified_schema_v12.sql");

    fn account(user: &str, device: &str, identity_key: &str) -> OlmAccountData {
        OlmAccountData {
            user_id: user.to_string(),
            device_id: device.to_string(),
            identity_key: identity_key.to_string(),
            serialized_account: format!("pickle-{identity_key}"),
            has_published_one_time_keys: false,
            has_published_fallback_key: false,
        }
    }

    fn session(user: &str, device: &str, session_id: &str, sender_key: &str, index: u32) -> OlmSessionData {
        OlmSessionData {
            session_id: session_id.to_string(),
            user_id: user.to_string(),
            device_id: device.to_string(),
            sender_key: sender_key.to_string(),
            receiver_key: format!("rkey-{session_id}"),
            serialized_state: format!("state-{session_id}"),
            message_index: index,
            created_ts: 1_000,
            last_used_ts: 1_000 + i64::from(index),
            expires_at: None,
        }
    }

    #[tokio::test]
    async fn test_olm_round_trip_on_migration_template() {
        let isolated = IsolatedTestPool::new(BASELINE_SQL).await.expect("isolated test pool");
        let pool = isolated.pool();
        let storage = OlmStorage::new(&pool);

        let user = "@c25:localhost";
        let device = "DEVICE25";

        // --- olm_accounts: save_account upserts on (user_id, device_id) ---
        storage.save_account(&account(user, device, "idkey-1")).await.unwrap();
        let loaded = storage.load_account(user, device).await.unwrap().expect("account row");
        assert_eq!(loaded.identity_key, "idkey-1");
        assert_eq!(loaded.serialized_account, "pickle-idkey-1");
        assert!(!loaded.has_published_one_time_keys);
        assert!(!loaded.has_published_fallback_key);
        assert!(storage.load_account(user, "OTHER-DEVICE").await.unwrap().is_none());

        let mut published = account(user, device, "idkey-2");
        published.has_published_one_time_keys = true;
        published.has_published_fallback_key = true;
        storage.save_account(&published).await.unwrap();

        let loaded = storage.load_account(user, device).await.unwrap().unwrap();
        assert_eq!(loaded.identity_key, "idkey-2", "save_account must update, not insert a second row");
        assert!(loaded.has_published_one_time_keys);
        assert!(loaded.has_published_fallback_key);
        let rows: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM olm_accounts WHERE user_id = $1")
            .bind(user)
            .fetch_one(&*pool)
            .await
            .unwrap();
        assert_eq!(rows, 1, "the UNIQUE (user_id, device_id) upsert must not duplicate");

        // `is_one_time_keys_published` / `is_fallback_key_published` are
        // `BOOLEAN DEFAULT FALSE` without NOT NULL, and `OlmAccountRow` takes them as
        // `Option<bool>`; `load_account` folds NULL to `false`. An explicit NULL must
        // reach that fold rather than fail the decode.
        sqlx::query(
            "INSERT INTO olm_accounts (user_id, device_id, identity_key, serialized_account, \
             is_one_time_keys_published, is_fallback_key_published, created_ts, updated_ts) \
             VALUES ($1, $2, $3, $4, NULL, NULL, $5, $5)",
        )
        .bind(user)
        .bind("NULL-FLAGS")
        .bind("idkey-null")
        .bind("pickle-null")
        .bind(0_i64)
        .execute(&*pool)
        .await
        .unwrap();
        let null_flags = storage.load_account(user, "NULL-FLAGS").await.unwrap().expect("NULL-flag account row");
        assert!(!null_flags.has_published_one_time_keys);
        assert!(!null_flags.has_published_fallback_key);

        // --- olm_sessions: save_session upserts on the UNIQUE session_id ---
        storage.save_session(&session(user, device, "sess-old", "sender-a", 5)).await.unwrap();
        storage.save_session(&session(user, device, "sess-new", "sender-a", 700_000)).await.unwrap();
        // Far-future expiry, so it exercises the `Some` round trip without being due
        // for `delete_expired_sessions` below.
        let mut optional_expiry = session(user, device, "sess-exp", "sender-b", 0);
        optional_expiry.expires_at = Some(9_000_000_000_000);
        storage.save_session(&optional_expiry).await.unwrap();

        // `ORDER BY last_used_ts DESC`: `sess-new` carries the larger index and
        // therefore the larger `last_used_ts`. `sess-exp`/`sess-old` tie at 1_000, so
        // only the head is pinned (a tie has no defined order).
        let sessions = storage.load_sessions(user, device).await.unwrap();
        assert_eq!(sessions.len(), 3);
        assert_eq!(sessions[0].session_id, "sess-new");
        // `message_index` is `INTEGER` (i32) in the row struct and `u32` in the model:
        // the 700_000 value must survive the widening.
        assert_eq!(sessions[0].message_index, 700_000);

        let old = storage.load_session("sess-old").await.unwrap().expect("sess-old");
        assert_eq!(old.message_index, 5);
        assert_eq!(old.serialized_state, "state-sess-old");
        assert_eq!(old.expires_at, None);
        assert!(storage.load_session("missing").await.unwrap().is_none());

        let by_sender = storage.load_session_by_sender_key(user, device, "sender-b").await.unwrap().expect("sender-b");
        assert_eq!(by_sender.session_id, "sess-exp");
        assert_eq!(by_sender.expires_at, Some(9_000_000_000_000));
        assert!(storage.load_session_by_sender_key(user, device, "sender-missing").await.unwrap().is_none());

        // get_session_count: `COUNT(*)` has no relation origin, so sqlx infers a
        // nullable column and the code folds it with `unwrap_or(0)` (C19a family).
        assert_eq!(storage.get_session_count(user, device).await.unwrap(), 3);
        assert_eq!(storage.get_session_count(user, "EMPTY-DEVICE").await.unwrap(), 0);

        // The re-save must rewrite the existing row, not add a fourth.
        let mut updated = session(user, device, "sess-old", "sender-a", 6);
        updated.serialized_state = "state-sess-old-v2".to_string();
        storage.save_session(&updated).await.unwrap();
        assert_eq!(storage.get_session_count(user, device).await.unwrap(), 3);
        let old = storage.load_session("sess-old").await.unwrap().unwrap();
        assert_eq!(old.message_index, 6);
        assert_eq!(old.serialized_state, "state-sess-old-v2");

        // --- update_session_last_used ---
        storage.update_session_last_used("sess-old").await.unwrap();
        let touched = storage.load_session("sess-old").await.unwrap().unwrap();
        assert!(touched.last_used_ts > 1_006, "last_used_ts must be refreshed, got {}", touched.last_used_ts);

        // --- delete_expired_sessions: only `expires_at IS NOT NULL AND < now` ---
        let mut expired = session(user, device, "sess-past", "sender-c", 0);
        expired.expires_at = Some(1);
        storage.save_session(&expired).await.unwrap();
        let mut future = session(user, device, "sess-future", "sender-d", 0);
        future.expires_at = Some(i64::MAX);
        storage.save_session(&future).await.unwrap();
        assert_eq!(storage.get_session_count(user, device).await.unwrap(), 5);

        assert_eq!(storage.delete_expired_sessions().await.unwrap(), 1, "only the past-expiry row is due");
        assert!(storage.load_session("sess-past").await.unwrap().is_none());
        // The boundary rows must survive: `expires_at IS NULL` (sess-old), a non-NULL
        // but not-yet-due expiry (sess-exp), and a far-future expiry (sess-future).
        assert!(storage.load_session("sess-exp").await.unwrap().is_some());
        assert!(storage.load_session("sess-future").await.unwrap().is_some());
        assert!(storage.load_session("sess-old").await.unwrap().is_some());
        assert_eq!(storage.get_session_count(user, device).await.unwrap(), 4);

        // --- delete_session / delete_sessions_for_device / delete_account ---
        storage.delete_session("sess-new").await.unwrap();
        assert!(storage.load_session("sess-new").await.unwrap().is_none());
        // Deleting an already-absent row is a no-op, not an error.
        storage.delete_session("sess-new").await.unwrap();

        // A session on another device must not be touched by the device-scoped delete.
        storage.save_account(&account(user, "OTHER-DEVICE", "idkey-other")).await.unwrap();
        storage.save_session(&session(user, "OTHER-DEVICE", "sess-other", "sender-e", 0)).await.unwrap();
        storage.delete_sessions_for_device(user, device).await.unwrap();
        assert_eq!(storage.get_session_count(user, device).await.unwrap(), 0);
        assert_eq!(storage.get_session_count(user, "OTHER-DEVICE").await.unwrap(), 1);

        // delete_account removes the account row and, through
        // delete_sessions_for_device, every session of that device.
        storage.delete_account(user, "OTHER-DEVICE").await.unwrap();
        assert!(storage.load_account(user, "OTHER-DEVICE").await.unwrap().is_none());
        assert_eq!(storage.get_session_count(user, "OTHER-DEVICE").await.unwrap(), 0);
        // The first device's account row is untouched by the other device's delete.
        assert!(storage.load_account(user, device).await.unwrap().is_some());
        storage.delete_account(user, device).await.unwrap();
        assert!(storage.load_account(user, device).await.unwrap().is_none());

        // --- D-49 (characterization): `message_index` is nullable in the schema ---
        // `olm_sessions.message_index` is `INTEGER DEFAULT 0` with no NOT NULL, while
        // `OlmSessionRow.message_index` is `i32` and the read projections assert
        // `AS "message_index!"`. No writer can currently produce NULL (the sole INSERT
        // always binds a non-Option value and `DEFAULT 0` covers omission), so the
        // mismatch is latent — but the schema still *accepts* an explicit NULL, and the
        // read path must then fail closed (Err) rather than coerce to 0.
        // When D-49 is fixed by `message_index INTEGER NOT NULL DEFAULT 0`, this INSERT
        // starts failing with 23502 and this block must be updated in step.
        sqlx::query(
            "INSERT INTO olm_sessions (user_id, device_id, session_id, sender_key, receiver_key, \
             serialized_state, message_index, created_ts, last_used_ts) \
             VALUES ($1, $2, $3, $4, $5, $6, NULL, $7, $7)",
        )
        .bind(user)
        .bind(device)
        .bind("sess-null-index")
        .bind("sender-null")
        .bind("rkey-null")
        .bind("state-null")
        .bind(0_i64)
        .execute(&*pool)
        .await
        .expect("D-49: the schema still accepts a NULL message_index");

        let err = storage
            .load_session("sess-null-index")
            .await
            .expect_err("D-49: a NULL message_index must fail closed, not decode as 0");
        assert_eq!(err.message, "Database error: Failed to load olm session");
    }
}

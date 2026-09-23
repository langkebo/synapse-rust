use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

/// The `RendezvousSession` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RendezvousSession {
    /// The `id` field.
    pub id: i64,
    /// The `session_id` field.
    pub session_id: String,
    /// The `user_id` field.
    pub user_id: Option<String>,
    /// The `device_id` field.
    pub device_id: Option<String>,
    /// The `intent` field.
    pub intent: Option<String>,
    /// The `transport` field.
    pub transport: Option<String>,
    /// The `transport_data` field.
    pub transport_data: Option<serde_json::Value>,
    /// The `key` field.
    pub key: Option<String>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `expires_at` field.
    pub expires_at: i64,
    /// The `status` field.
    pub status: Option<String>,
}

/// The `RendezvousIntent` enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RendezvousIntent {
    #[serde(rename = "login.reciprocate")]
    /// The `LoginReciprocate` variant.
    LoginReciprocate,
    #[serde(rename = "login.start")]
    /// The `LoginStart` variant.
    LoginStart,
}

impl RendezvousIntent {
    /// See [`as_str`].
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LoginReciprocate => "login.reciprocate",
            Self::LoginStart => "login.start",
        }
    }
}

/// The `RendezvousTransport` enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RendezvousTransport {
    #[serde(rename = "http.v1")]
    /// The `HttpV1` variant.
    HttpV1,
    #[serde(rename = "http.v2")]
    /// The `HttpV2` variant.
    HttpV2,
}

impl RendezvousTransport {
    /// See [`as_str`].
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HttpV1 => "http.v1",
            Self::HttpV2 => "http.v2",
        }
    }
}

/// The `CreateRendezvousSessionParams` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRendezvousSessionParams {
    /// The `intent` field.
    pub intent: RendezvousIntent,
    /// The `transport` field.
    pub transport: RendezvousTransport,
    /// The `transport_data` field.
    pub transport_data: Option<serde_json::Value>,
    /// The `expires_in_ms` field.
    pub expires_in_ms: Option<i64>,
}

/// The `RendezvousCode` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendezvousCode {
    /// The `url` field.
    pub url: String,
    /// The `session_id` field.
    pub session_id: String,
    /// The `key` field.
    pub key: String,
}

/// The `RendezvousMessage` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendezvousMessage {
    #[serde(rename = "type")]
    /// The `message_type` field.
    pub message_type: String,
    /// The `content` field.
    pub content: serde_json::Value,
}

/// The `RendezvousLoginStart` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendezvousLoginStart {
    /// The `homeserver` field.
    pub homeserver: String,
    /// The `user` field.
    pub user: Option<RendezvousLoginUser>,
}

/// The `RendezvousLoginUser` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendezvousLoginUser {
    /// The `user_id` field.
    pub user_id: String,
    /// The `display_name` field.
    pub display_name: Option<String>,
    /// The `device_id` field.
    pub device_id: String,
}

/// The `RendezvousLoginFinish` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendezvousLoginFinish {
    /// The `access_token` field.
    pub access_token: String,
    /// The `device_id` field.
    pub device_id: String,
    /// The `user_id` field.
    pub user_id: String,
}

/// Outcome of an MSC4108 conditional update. MSC4108 requires distinguishing
/// "session not found / expired" (404 `M_NOT_FOUND`) from "ETag precondition
/// failed" (412 with unstable `org.matrix.msc4108.errcode = M_CONCURRENT_WRITE`)
/// — the old `Option<String>` signature conflated them.
///
/// Every successful / failed variant carries the session's current
/// `updated_ts` and `expires_at` so the route can emit the required
/// `Last-Modified` / `Expires` headers (per MSC4108 §"Common HTTP response
/// headers") even on 412.
#[derive(Debug, Clone)]
pub enum Msc4108UpdateOutcome {
    /// Session row is missing or expired — caller should respond 404 `M_NOT_FOUND`.
    NotFound,
    /// `If-Match` was supplied and did not match the current row's etag.
    /// Contains the current etag / `updated_ts` / `expires_at` so the caller
    /// can emit a spec-compliant 412 with the required common headers.
    PreconditionFailed {
        /// Current strong ETag of the session payload.
        current_etag: String,
        /// Last-modified timestamp (millis) of the current payload.
        updated_ts: i64,
        /// Session absolute expiry (millis).
        expires_at: i64,
    },
    /// Update succeeded. Contains the new etag / `updated_ts` / `expires_at`.
    Updated {
        /// New strong ETag of the written payload.
        new_etag: String,
        /// Writes' timestamp (millis) — becomes the new `Last-Modified`.
        updated_ts: i64,
        /// Session absolute expiry (millis).
        expires_at: i64,
    },
}

/// The `RendezvousStoreApi` trait.
#[async_trait]
pub trait RendezvousStoreApi: Send + Sync {
    /// See [`create_session`].
    async fn create_session(&self, params: CreateRendezvousSessionParams) -> Result<RendezvousSession, sqlx::Error>;
    /// See [`get_session`].
    async fn get_session(&self, session_id: &str) -> Result<Option<RendezvousSession>, sqlx::Error>;
    /// See [`update_session_status`].
    async fn update_session_status(&self, session_id: &str, status: &str) -> Result<(), sqlx::Error>;
    /// See [`bind_user_to_session`].
    async fn bind_user_to_session(&self, session_id: &str, user_id: &str, device_id: &str) -> Result<(), sqlx::Error>;
    /// See [`complete_session`].
    async fn complete_session(&self, session_id: &str) -> Result<(), sqlx::Error>;
    /// See [`delete_session`].
    async fn delete_session(&self, session_id: &str) -> Result<(), sqlx::Error>;
    /// See [`cleanup_expired_sessions`].
    async fn cleanup_expired_sessions(&self) -> Result<u64, sqlx::Error>;
    /// See [`store_message`].
    async fn store_message(
        &self,
        session_id: &str,
        direction: &str,
        message: &RendezvousMessage,
    ) -> Result<(), sqlx::Error>;
    /// See [`get_messages`].
    async fn get_messages(
        &self,
        session_id: &str,
        after_id: Option<i64>,
    ) -> Result<Vec<StoredRendezvousMessage>, sqlx::Error>;

    // ── MSC4108 methods ──
    /// See [`create_msc4108_session`].
    ///
    /// Returns `(session_id, etag, created_ts, expires_at)`. `created_ts` is the
    /// row's `updated_ts` at creation time and feeds the required
    /// `Last-Modified` response header; `expires_at` feeds `Expires`.
    async fn create_msc4108_session(
        &self,
        initial_data: &str,
        ttl_ms: i64,
    ) -> Result<(String, String, i64, i64), sqlx::Error>;
    /// See [`get_msc4108_data`].
    ///
    /// Returns `(data, etag, updated_ts, expires_at)` or None if not found/expired.
    async fn get_msc4108_data(&self, session_id: &str) -> Result<Option<(String, String, i64, i64)>, sqlx::Error>;
    /// See [`update_msc4108_data`].
    ///
    /// Distinguishes success / ETag mismatch / not-found via [`Msc4108UpdateOutcome`]
    /// so the route can return 202 / 412 / 404 respectively per MSC4108.
    async fn update_msc4108_data(
        &self,
        session_id: &str,
        data: &str,
        if_match: Option<&str>,
    ) -> Result<Msc4108UpdateOutcome, sqlx::Error>;
    /// See [`delete_msc4108_session`].
    ///
    /// Returns `true` when a session row was deleted, `false` when none existed —
    /// per MSC4108 the caller turns `false` into 404 `M_NOT_FOUND`.
    async fn delete_msc4108_session(&self, session_id: &str) -> Result<bool, sqlx::Error>;
}

/// The `RendezvousStorage` struct.
#[derive(Clone)]
pub struct RendezvousStorage {
    /// The `pool` field.
    pub pool: Arc<Pool<Postgres>>,
}

impl RendezvousStorage {
    /// See [`new`].
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    /// See [`create_session`].
    pub async fn create_session(
        &self,
        params: CreateRendezvousSessionParams,
    ) -> Result<RendezvousSession, sqlx::Error> {
        let now = current_timestamp_millis();
        let session_id = uuid::Uuid::new_v4().simple().to_string().to_string();
        let key = Self::generate_key();
        let expires_at = now + params.expires_in_ms.unwrap_or(5 * 60 * 1000);

        sqlx::query_as::<_, RendezvousSession>(
            r"
            INSERT INTO rendezvous_session
                (session_id, intent, transport, transport_data, key, created_ts, expires_at, status)
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending')
            RETURNING *
            ",
        )
        .bind(&session_id)
        .bind(params.intent.as_str())
        .bind(params.transport.as_str())
        .bind(&params.transport_data)
        .bind(&key)
        .bind(now)
        .bind(expires_at)
        .fetch_one(&*self.pool)
        .await
    }

    /// See [`get_session`].
    pub async fn get_session(&self, session_id: &str) -> Result<Option<RendezvousSession>, sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query_as::<_, RendezvousSession>(
            r"
            SELECT id, session_id, user_id, device_id, intent, transport, transport_data, key, created_ts, expires_at, status FROM rendezvous_session
            WHERE session_id = $1 AND expires_at > $2
            ",
        )
        .bind(session_id)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await
    }

    /// See [`update_session_status`].
    pub async fn update_session_status(&self, session_id: &str, status: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE rendezvous_session
            SET status = $2
            WHERE session_id = $1
            ",
        )
        .bind(session_id)
        .bind(status)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`bind_user_to_session`].
    pub async fn bind_user_to_session(
        &self,
        session_id: &str,
        user_id: &str,
        device_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE rendezvous_session
            SET user_id = $2, device_id = $3, status = 'connected'
            WHERE session_id = $1
            ",
        )
        .bind(session_id)
        .bind(user_id)
        .bind(device_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`complete_session`].
    pub async fn complete_session(&self, session_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE rendezvous_session
            SET status = 'completed'
            WHERE session_id = $1
            ",
        )
        .bind(session_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`delete_session`].
    pub async fn delete_session(&self, session_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            DELETE FROM rendezvous_session WHERE session_id = $1
            ",
        )
        .bind(session_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`cleanup_expired_sessions`].
    pub async fn cleanup_expired_sessions(&self) -> Result<u64, sqlx::Error> {
        let now = current_timestamp_millis();

        let result = sqlx::query(
            r"
            DELETE FROM rendezvous_session WHERE expires_at < $1
            ",
        )
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    fn generate_key() -> String {
        use rand::RngCore;
        let mut key_bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut key_bytes);
        URL_SAFE_NO_PAD.encode(key_bytes)
    }

    /// See [`store_message`].
    pub async fn store_message(
        &self,
        session_id: &str,
        direction: &str,
        message: &RendezvousMessage,
    ) -> Result<(), sqlx::Error> {
        RendezvousMessageStorage::new(self.pool.clone()).store_message(session_id, direction, message).await
    }

    /// See [`get_messages`].
    pub async fn get_messages(
        &self,
        session_id: &str,
        after_id: Option<i64>,
    ) -> Result<Vec<StoredRendezvousMessage>, sqlx::Error> {
        RendezvousMessageStorage::new(self.pool.clone()).get_messages(session_id, after_id).await
    }

    // ── MSC4108 methods ───────────────────────────────────────────────────
    // MSC4108 uses text/plain ETag-based polling, storing opaque encrypted blobs.
    // We reuse the `content` JSONB column to store `{"data": "<base64_text>"}`.

    /// Create a new MSC4108 rendezvous session with initial data.
    /// Returns `(session_id, etag, created_ts, expires_at_millis)` — the last two
    /// feed the required `Last-Modified` / `Expires` response headers (MSC4108
    /// §Common HTTP response headers).
    pub async fn create_msc4108_session(
        &self,
        initial_data: &str,
        ttl_ms: i64,
    ) -> Result<(String, String, i64, i64), sqlx::Error> {
        let now = current_timestamp_millis();
        let session_id = uuid::Uuid::new_v4().simple().to_string();
        let expires_at = now + ttl_ms;
        let content = serde_json::json!({ "data": initial_data });

        sqlx::query(
            r"
            INSERT INTO rendezvous_session
                (session_id, intent, transport, content, key, created_ts, updated_ts, expires_at, status)
            VALUES ($1, 'msc4108', 'http', $2, $3, $4, $4, $5, 'active')
            ",
        )
        .bind(&session_id)
        .bind(&content)
        .bind(Self::generate_key())
        .bind(now)
        .bind(expires_at)
        .execute(&*self.pool)
        .await?;

        let etag = format!("\"{now}\"");
        Ok((session_id, etag, now, expires_at))
    }

    /// Get MSC4108 session data. Returns `(data, etag, updated_ts, expires_at)`
    /// or None if not found/expired. `updated_ts`/`expires_at` feed the
    /// required `Last-Modified`/`Expires` response headers.
    pub async fn get_msc4108_data(&self, session_id: &str) -> Result<Option<(String, String, i64, i64)>, sqlx::Error> {
        let now = current_timestamp_millis();
        let row: Option<(serde_json::Value, Option<i64>, i64)> = sqlx::query_as(
            r"
            SELECT content, updated_ts, expires_at FROM rendezvous_session
            WHERE session_id = $1 AND intent = 'msc4108' AND expires_at > $2
            ",
        )
        .bind(session_id)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await?;

        match row {
            Some((content, updated_ts, expires_at)) => {
                let data = content.get("data").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let updated = updated_ts.unwrap_or(0);
                let etag = format!("\"{updated}\"");
                Ok(Some((data, etag, updated, expires_at)))
            }
            None => Ok(None),
        }
    }

    /// Update MSC4108 session data, distinguishing success / precondition
    /// failure / not-found via [`Msc4108UpdateOutcome`].
    ///
    /// Uses a `SELECT ... FOR UPDATE` transaction so the precondition check and
    /// the write are atomic (MSC4108's `M_CONCURRENT_WRITE` semantics assume no
    /// lost-update race between the `If-Match` check and the payload swap).
    pub async fn update_msc4108_data(
        &self,
        session_id: &str,
        data: &str,
        if_match: Option<&str>,
    ) -> Result<Msc4108UpdateOutcome, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let now = current_timestamp_millis();

        let current: Option<(Option<i64>, i64)> = sqlx::query_as(
            r"
            SELECT updated_ts, expires_at FROM rendezvous_session
            WHERE session_id = $1 AND intent = 'msc4108'
            FOR UPDATE
            ",
        )
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await?;

        let (current_updated, expires_at) = match current {
            Some((updated_ts, expires_at)) if expires_at > now => (updated_ts.unwrap_or(0), expires_at),
            // Missing row or expired session — 404 M_NOT_FOUND per MSC4108.
            _ => {
                tx.rollback().await?;
                return Ok(Msc4108UpdateOutcome::NotFound);
            }
        };

        if let Some(expected_etag) = if_match {
            let expected_raw = expected_etag.trim_matches('"');
            if expected_raw != current_updated.to_string() {
                tx.rollback().await?;
                return Ok(Msc4108UpdateOutcome::PreconditionFailed {
                    current_etag: format!("\"{current_updated}\""),
                    updated_ts: current_updated,
                    expires_at,
                });
            }
        }

        let content = serde_json::json!({ "data": data });
        sqlx::query(
            r"
            UPDATE rendezvous_session
            SET content = $2, updated_ts = $3
            WHERE session_id = $1 AND intent = 'msc4108' AND expires_at > $3
            ",
        )
        .bind(session_id)
        .bind(&content)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(Msc4108UpdateOutcome::Updated { new_etag: format!("\"{now}\""), updated_ts: now, expires_at })
    }

    /// Delete an MSC4108 session. Returns whether a row was actually removed —
    /// MSC4108 requires 404 `M_NOT_FOUND` for unknown/expired session ids,
    /// which the route derives from `false` here.
    pub async fn delete_msc4108_session(&self, session_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM rendezvous_session WHERE session_id = $1 AND intent = 'msc4108'")
            .bind(session_id)
            .execute(&*self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[async_trait]
impl RendezvousStoreApi for RendezvousStorage {
    async fn create_session(&self, params: CreateRendezvousSessionParams) -> Result<RendezvousSession, sqlx::Error> {
        self.create_session(params).await
    }
    async fn get_session(&self, session_id: &str) -> Result<Option<RendezvousSession>, sqlx::Error> {
        self.get_session(session_id).await
    }
    async fn update_session_status(&self, session_id: &str, status: &str) -> Result<(), sqlx::Error> {
        self.update_session_status(session_id, status).await
    }
    async fn bind_user_to_session(&self, session_id: &str, user_id: &str, device_id: &str) -> Result<(), sqlx::Error> {
        self.bind_user_to_session(session_id, user_id, device_id).await
    }
    async fn complete_session(&self, session_id: &str) -> Result<(), sqlx::Error> {
        self.complete_session(session_id).await
    }
    async fn delete_session(&self, session_id: &str) -> Result<(), sqlx::Error> {
        self.delete_session(session_id).await
    }
    async fn cleanup_expired_sessions(&self) -> Result<u64, sqlx::Error> {
        self.cleanup_expired_sessions().await
    }
    async fn store_message(
        &self,
        session_id: &str,
        direction: &str,
        message: &RendezvousMessage,
    ) -> Result<(), sqlx::Error> {
        self.store_message(session_id, direction, message).await
    }
    async fn get_messages(
        &self,
        session_id: &str,
        after_id: Option<i64>,
    ) -> Result<Vec<StoredRendezvousMessage>, sqlx::Error> {
        self.get_messages(session_id, after_id).await
    }
    async fn create_msc4108_session(
        &self,
        initial_data: &str,
        ttl_ms: i64,
    ) -> Result<(String, String, i64, i64), sqlx::Error> {
        self.create_msc4108_session(initial_data, ttl_ms).await
    }
    async fn get_msc4108_data(&self, session_id: &str) -> Result<Option<(String, String, i64, i64)>, sqlx::Error> {
        self.get_msc4108_data(session_id).await
    }
    async fn update_msc4108_data(
        &self,
        session_id: &str,
        data: &str,
        if_match: Option<&str>,
    ) -> Result<Msc4108UpdateOutcome, sqlx::Error> {
        self.update_msc4108_data(session_id, data, if_match).await
    }
    async fn delete_msc4108_session(&self, session_id: &str) -> Result<bool, sqlx::Error> {
        self.delete_msc4108_session(session_id).await
    }
}

/// The `RendezvousMessageStorage` struct.
#[derive(Clone)]
pub struct RendezvousMessageStorage {
    pool: Arc<Pool<Postgres>>,
}

/// The `StoredRendezvousMessage` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StoredRendezvousMessage {
    /// The `id` field.
    pub id: i64,
    /// The `session_id` field.
    pub session_id: String,
    /// The `direction` field.
    pub direction: String,
    /// The `message_type` field.
    pub message_type: String,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `created_ts` field.
    pub created_ts: i64,
}

impl RendezvousMessageStorage {
    /// See [`new`].
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    /// See [`store_message`].
    pub async fn store_message(
        &self,
        session_id: &str,
        direction: &str,
        message: &RendezvousMessage,
    ) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();

        sqlx::query(
            r"
            INSERT INTO rendezvous_messages
                (session_id, direction, message_type, content, created_ts)
            VALUES ($1, $2, $3, $4, $5)
            ",
        )
        .bind(session_id)
        .bind(direction)
        .bind(&message.message_type)
        .bind(&message.content)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// See [`get_messages`].
    pub async fn get_messages(
        &self,
        session_id: &str,
        after_id: Option<i64>,
    ) -> Result<Vec<StoredRendezvousMessage>, sqlx::Error> {
        match after_id {
            Some(after) => {
                sqlx::query_as::<_, StoredRendezvousMessage>(
                    r"
                    SELECT id, session_id, direction, message_type, content, created_ts FROM rendezvous_messages
                    WHERE session_id = $1 AND id > $2
                    ORDER BY id ASC
                    ",
                )
                .bind(session_id)
                .bind(after)
                .fetch_all(&*self.pool)
                .await
            }
            None => {
                sqlx::query_as::<_, StoredRendezvousMessage>(
                    r"
                    SELECT id, session_id, direction, message_type, content, created_ts FROM rendezvous_messages
                    WHERE session_id = $1
                    ORDER BY id ASC
                    ",
                )
                .bind(session_id)
                .fetch_all(&*self.pool)
                .await
            }
        }
    }

    /// See [`delete_messages`].
    pub async fn delete_messages(&self, session_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            DELETE FROM rendezvous_messages WHERE session_id = $1
            ",
        )
        .bind(session_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rendezvous_intent() {
        assert_eq!(RendezvousIntent::LoginReciprocate.as_str(), "login.reciprocate");
        assert_eq!(RendezvousIntent::LoginStart.as_str(), "login.start");
    }

    #[test]
    fn test_rendezvous_transport() {
        assert_eq!(RendezvousTransport::HttpV1.as_str(), "http.v1");
        assert_eq!(RendezvousTransport::HttpV2.as_str(), "http.v2");
    }

    #[test]
    fn test_create_params() {
        let params = CreateRendezvousSessionParams {
            intent: RendezvousIntent::LoginReciprocate,
            transport: RendezvousTransport::HttpV1,
            transport_data: Some(serde_json::json!({"url": "https://example.com/rendezvous"})),
            expires_in_ms: Some(300000),
        };

        assert_eq!(params.intent.as_str(), "login.reciprocate");
        assert!(params.transport_data.is_some());
    }

    #[test]
    fn test_rendezvous_code() {
        let code = RendezvousCode {
            url: "https://example.com/rendezvous/abc123".to_string(),
            session_id: "abc123".to_string(),
            key: "base64_encoded_key".to_string(),
        };

        assert_eq!(code.session_id, "abc123");
        assert!(code.url.contains("abc123"));
    }

    #[test]
    fn test_rendezvous_message() {
        let message = RendezvousMessage {
            message_type: "m.login.start".to_string(),
            content: serde_json::json!({"homeserver": "https://matrix.example.com"}),
        };

        assert_eq!(message.message_type, "m.login.start");
    }

    #[test]
    fn test_rendezvous_login_start() {
        let login_start = RendezvousLoginStart {
            homeserver: "https://matrix.example.com".to_string(),
            user: Some(RendezvousLoginUser {
                user_id: "@alice:example.com".to_string(),
                display_name: Some("Alice".to_string()),
                device_id: "DEVICE123".to_string(),
            }),
        };

        assert_eq!(login_start.homeserver, "https://matrix.example.com");
        assert!(login_start.user.is_some());
    }

    #[test]
    fn test_rendezvous_login_finish() {
        let login_finish = RendezvousLoginFinish {
            access_token: "syt_abc123".to_string(),
            device_id: "DEVICE456".to_string(),
            user_id: "@bob:example.com".to_string(),
        };

        assert_eq!(login_finish.access_token, "syt_abc123");
        assert_eq!(login_finish.user_id, "@bob:example.com");
    }

    #[test]
    fn test_session_status_values() {
        let statuses = vec!["waiting", "connected", "completed", "cancelled", "expired"];

        for status in statuses {
            assert!(!status.is_empty());
        }
    }

    #[test]
    fn test_message_direction_values() {
        let directions = vec!["incoming", "outgoing"];

        for direction in directions {
            assert!(!direction.is_empty());
        }
    }

    #[test]
    fn test_generate_key() {
        let key1 = RendezvousStorage::generate_key();
        let key2 = RendezvousStorage::generate_key();

        assert_ne!(key1, key2);
        assert!(key1.len() > 30);
    }

    #[test]
    fn test_create_params_with_all_fields() {
        let params = CreateRendezvousSessionParams {
            intent: RendezvousIntent::LoginStart,
            transport: RendezvousTransport::HttpV2,
            transport_data: Some(serde_json::json!({
                "url": "https://example.com/rendezvous",
                "token": "abc123"
            })),
            expires_in_ms: Some(600000),
        };

        assert_eq!(params.intent.as_str(), "login.start");
        assert_eq!(params.transport.as_str(), "http.v2");
        assert!(params.transport_data.is_some());
        assert_eq!(params.expires_in_ms, Some(600000));
    }

    #[test]
    fn test_create_params_with_minimal_fields() {
        let params = CreateRendezvousSessionParams {
            intent: RendezvousIntent::LoginReciprocate,
            transport: RendezvousTransport::HttpV1,
            transport_data: None,
            expires_in_ms: None,
        };

        assert_eq!(params.intent.as_str(), "login.reciprocate");
        assert!(params.transport_data.is_none());
        assert!(params.expires_in_ms.is_none());
    }

    #[test]
    fn test_rendezvous_code_structure() {
        let code = RendezvousCode {
            url: "https://example.com/rendezvous/xyz789".to_string(),
            session_id: "xyz789".to_string(),
            key: "base64encodedkey".to_string(),
        };

        assert!(code.url.contains(code.session_id.as_str()));
        assert!(!code.key.is_empty());
    }

    #[test]
    fn test_rendezvous_message_with_complex_content() {
        let message = RendezvousMessage {
            message_type: "m.login.finish".to_string(),
            content: serde_json::json!({
                "credentials": {
                    "username": "alice",
                    "password_hash": "hashed_value"
                },
                "device_id": "DEVICE123"
            }),
        };

        assert_eq!(message.message_type, "m.login.finish");
        assert!(message.content.get("credentials").is_some());
    }

    #[test]
    fn test_rendezvous_login_user_with_display_name() {
        let user = RendezvousLoginUser {
            user_id: "@alice:example.com".to_string(),
            display_name: Some("Alice Wonderland".to_string()),
            device_id: "DEVICE789".to_string(),
        };

        assert!(user.display_name.is_some());
        assert_eq!(user.display_name.unwrap(), "Alice Wonderland");
    }

    #[test]
    fn test_rendezvous_login_user_without_display_name() {
        let user = RendezvousLoginUser {
            user_id: "@bob:example.com".to_string(),
            display_name: None,
            device_id: "DEVICE456".to_string(),
        };

        assert!(user.display_name.is_none());
        assert_eq!(user.user_id, "@bob:example.com");
    }

    #[test]
    fn test_msc4108_update_outcome_not_found() {
        let outcome = Msc4108UpdateOutcome::NotFound;

        assert!(matches!(outcome, Msc4108UpdateOutcome::NotFound));
    }

    #[test]
    fn test_msc4108_update_outcome_precondition_failed() {
        let outcome = Msc4108UpdateOutcome::PreconditionFailed {
            current_etag: "\"1234567890\"".to_string(),
            updated_ts: 1234567890,
            expires_at: 1234567890 + 300000,
        };

        // `matches!` with a guard keeps the assertions without a `panic!` arm —
        // `clippy::panic` is denied crate-wide, and "unreachable" arms are how that
        // lint gets tripped in test code.
        assert!(
            matches!(
                outcome,
                Msc4108UpdateOutcome::PreconditionFailed { ref current_etag, updated_ts, expires_at }
                    if current_etag == "\"1234567890\""
                        && updated_ts == 1234567890
                        && expires_at == 1234567890 + 300000
            ),
            "expected PreconditionFailed carrying the constructed fields"
        );
    }

    #[test]
    fn test_msc4108_update_outcome_updated() {
        let outcome = Msc4108UpdateOutcome::Updated {
            new_etag: "\"9876543210\"".to_string(),
            updated_ts: 9876543210,
            expires_at: 9876543210 + 600000,
        };

        assert!(
            matches!(
                outcome,
                Msc4108UpdateOutcome::Updated { ref new_etag, updated_ts, expires_at }
                    if new_etag == "\"9876543210\""
                        && updated_ts == 9876543210
                        && expires_at == 9876543210 + 600000
            ),
            "expected Updated carrying the constructed fields"
        );
    }

    #[test]
    fn test_stored_rendezvous_message_structure() {
        let message = StoredRendezvousMessage {
            id: 1,
            session_id: "session123".to_string(),
            direction: "incoming".to_string(),
            message_type: "m.login.start".to_string(),
            content: serde_json::json!({"homeserver": "https://matrix.example.com"}),
            created_ts: 1234567890,
        };

        assert_eq!(message.id, 1);
        assert_eq!(message.direction, "incoming");
        assert!(message.created_ts > 0);
    }

    #[test]
    fn test_rendezvous_session_optional_fields() {
        let session = RendezvousSession {
            id: 1,
            session_id: "test-session".to_string(),
            user_id: Some("@user:example.com".to_string()),
            device_id: Some("DEVICE123".to_string()),
            intent: Some("login.start".to_string()),
            transport: Some("http.v1".to_string()),
            transport_data: Some(serde_json::json!({"url": "https://example.com"})),
            key: Some("base64key".to_string()),
            created_ts: 1234567890,
            expires_at: 1234567890 + 300000,
            status: Some("pending".to_string()),
        };

        assert!(session.user_id.is_some());
        assert!(session.device_id.is_some());
        assert_eq!(session.status, Some("pending".to_string()));
    }

    #[test]
    fn test_rendezvous_session_minimal_fields() {
        let session = RendezvousSession {
            id: 1,
            session_id: "minimal-session".to_string(),
            user_id: None,
            device_id: None,
            intent: None,
            transport: None,
            transport_data: None,
            key: None,
            created_ts: 1234567890,
            expires_at: 1234567890 + 300000,
            status: None,
        };

        assert!(session.user_id.is_none());
        assert!(session.device_id.is_none());
        assert!(session.intent.is_none());
        assert_eq!(session.status, None);
    }
}

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
    async fn create_msc4108_session(
        &self,
        initial_data: &str,
        ttl_ms: i64,
    ) -> Result<(String, String, i64), sqlx::Error>;
    /// See [`get_msc4108_data`].
    async fn get_msc4108_data(&self, session_id: &str) -> Result<Option<(String, String)>, sqlx::Error>;
    /// See [`update_msc4108_data`].
    async fn update_msc4108_data(
        &self,
        session_id: &str,
        data: &str,
        if_match: Option<&str>,
    ) -> Result<Option<String>, sqlx::Error>;
    /// See [`delete_msc4108_session`].
    async fn delete_msc4108_session(&self, session_id: &str) -> Result<(), sqlx::Error>;
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
    /// Returns (session_id, etag, expires_at_millis).
    pub async fn create_msc4108_session(
        &self,
        initial_data: &str,
        ttl_ms: i64,
    ) -> Result<(String, String, i64), sqlx::Error> {
        let now = current_timestamp_millis();
        let session_id = uuid::Uuid::new_v4().simple().to_string().to_string();
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

        let etag = format!("\"{}\"", now);
        Ok((session_id, etag, expires_at))
    }

    /// Get MSC4108 session data. Returns (data, etag) or None if not found/expired.
    pub async fn get_msc4108_data(&self, session_id: &str) -> Result<Option<(String, String)>, sqlx::Error> {
        let now = current_timestamp_millis();
        let row: Option<(serde_json::Value, Option<i64>)> = sqlx::query_as(
            r"
            SELECT content, updated_ts FROM rendezvous_session
            WHERE session_id = $1 AND intent = 'msc4108' AND expires_at > $2
            ",
        )
        .bind(session_id)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await?;

        match row {
            Some((content, updated_ts)) => {
                let data = content.get("data").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let etag = format!("\"{}\"", updated_ts.unwrap_or(0));
                Ok(Some((data, etag)))
            }
            None => Ok(None),
        }
    }

    /// Update MSC4108 session data. Returns new etag, or None if session not found.
    /// If `if_match` is provided, returns None when etag doesn't match (conditional update failed).
    pub async fn update_msc4108_data(
        &self,
        session_id: &str,
        data: &str,
        if_match: Option<&str>,
    ) -> Result<Option<String>, sqlx::Error> {
        let now = current_timestamp_millis();
        let content = serde_json::json!({ "data": data });

        // Check etag if provided
        if let Some(expected_etag) = if_match {
            let expected_ts: String = expected_etag.trim_matches('"').to_string();
            let exists: Option<bool> = sqlx::query_scalar(
                r"
                SELECT EXISTS(SELECT 1 FROM rendezvous_session
                WHERE session_id = $1 AND intent = 'msc4108'
                AND updated_ts::TEXT = $2 AND expires_at > $3)
                ",
            )
            .bind(session_id)
            .bind(&expected_ts)
            .bind(now)
            .fetch_optional(&*self.pool)
            .await?;

            if exists != Some(true) {
                return Ok(None); // ETag mismatch or session not found
            }
        }

        let result = sqlx::query(
            r"
            UPDATE rendezvous_session
            SET content = $2, updated_ts = $3
            WHERE session_id = $1 AND intent = 'msc4108' AND expires_at > $3
            ",
        )
        .bind(session_id)
        .bind(&content)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        if result.rows_affected() == 0 {
            Ok(None)
        } else {
            Ok(Some(format!("\"{}\"", now)))
        }
    }

    /// Delete an MSC4108 session.
    pub async fn delete_msc4108_session(&self, session_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM rendezvous_session WHERE session_id = $1 AND intent = 'msc4108'")
            .bind(session_id)
            .execute(&*self.pool)
            .await?;
        Ok(())
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
    ) -> Result<(String, String, i64), sqlx::Error> {
        self.create_msc4108_session(initial_data, ttl_ms).await
    }
    async fn get_msc4108_data(&self, session_id: &str) -> Result<Option<(String, String)>, sqlx::Error> {
        self.get_msc4108_data(session_id).await
    }
    async fn update_msc4108_data(
        &self,
        session_id: &str,
        data: &str,
        if_match: Option<&str>,
    ) -> Result<Option<String>, sqlx::Error> {
        self.update_msc4108_data(session_id, data, if_match).await
    }
    async fn delete_msc4108_session(&self, session_id: &str) -> Result<(), sqlx::Error> {
        self.delete_msc4108_session(session_id).await
    }
}

/// The `RendezvousMessageStoreApi` trait.
#[async_trait]
pub trait RendezvousMessageStoreApi: Send + Sync {
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
    /// See [`delete_messages`].
    async fn delete_messages(&self, session_id: &str) -> Result<(), sqlx::Error>;
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

#[async_trait]
impl RendezvousMessageStoreApi for RendezvousMessageStorage {
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
    async fn delete_messages(&self, session_id: &str) -> Result<(), sqlx::Error> {
        self.delete_messages(session_id).await
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
}

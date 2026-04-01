use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RendezvousSession {
    pub id: i64,
    pub session_id: String,
    pub user_id: Option<String>,
    pub device_id: Option<String>,
    pub intent: String,
    pub transport: String,
    pub transport_data: Option<serde_json::Value>,
    pub key: String,
    pub created_ts: i64,
    #[sqlx(rename = "expires_ts")]
    pub expires_at: i64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RendezvousIntent {
    #[serde(rename = "login.reciprocate")]
    LoginReciprocate,
    #[serde(rename = "login.start")]
    LoginStart,
}

impl RendezvousIntent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LoginReciprocate => "login.reciprocate",
            Self::LoginStart => "login.start",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RendezvousTransport {
    #[serde(rename = "http.v1")]
    HttpV1,
    #[serde(rename = "http.v2")]
    HttpV2,
}

impl RendezvousTransport {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HttpV1 => "http.v1",
            Self::HttpV2 => "http.v2",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRendezvousSessionParams {
    pub intent: RendezvousIntent,
    pub transport: RendezvousTransport,
    pub transport_data: Option<serde_json::Value>,
    pub expires_in_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendezvousCode {
    pub url: String,
    pub session_id: String,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendezvousMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub content: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendezvousLoginStart {
    pub homeserver: String,
    pub user: Option<RendezvousLoginUser>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendezvousLoginUser {
    pub user_id: String,
    pub display_name: Option<String>,
    pub device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendezvousLoginFinish {
    pub access_token: String,
    pub device_id: String,
    pub user_id: String,
}

#[derive(Clone)]
pub struct RendezvousStorage {
    pub pool: Arc<Pool<Postgres>>,
}

impl RendezvousStorage {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    pub async fn create_session(
        &self,
        params: CreateRendezvousSessionParams,
    ) -> Result<RendezvousSession, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let session_id = uuid::Uuid::new_v4().simple().to_string();
        let key = Self::generate_key();
        let expires_at = now + params.expires_in_ms.unwrap_or(5 * 60 * 1000);

        sqlx::query_as::<_, RendezvousSession>(
            r#"
            INSERT INTO rendezvous_session
                (session_id, intent, transport, transport_data, key, created_ts, expires_ts, status)
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'waiting')
            RETURNING *
            "#,
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

    pub async fn get_session(
        &self,
        session_id: &str,
    ) -> Result<Option<RendezvousSession>, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query_as::<_, RendezvousSession>(
            r#"
            SELECT * FROM rendezvous_session
            WHERE session_id = $1 AND expires_ts > $2
            "#,
        )
        .bind(session_id)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn update_session_status(
        &self,
        session_id: &str,
        status: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE rendezvous_session 
            SET status = $2
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .bind(status)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn bind_user_to_session(
        &self,
        session_id: &str,
        user_id: &str,
        device_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE rendezvous_session 
            SET user_id = $2, device_id = $3, status = 'connected'
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .bind(user_id)
        .bind(device_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn complete_session(&self, session_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE rendezvous_session 
            SET status = 'completed'
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM rendezvous_session WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn cleanup_expired_sessions(&self) -> Result<u64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let result = sqlx::query(
            r#"
            DELETE FROM rendezvous_session WHERE expires_ts < $1
            "#,
        )
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    fn generate_key() -> String {
        use rand::RngCore;
        let mut key_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key_bytes);
        URL_SAFE_NO_PAD.encode(key_bytes)
    }
}

#[derive(Clone)]
pub struct RendezvousMessageStorage {
    pool: Arc<Pool<Postgres>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StoredRendezvousMessage {
    pub id: i64,
    pub session_id: String,
    pub direction: String,
    pub message_type: String,
    pub content: serde_json::Value,
    pub created_ts: i64,
}

impl RendezvousMessageStorage {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    pub async fn store_message(
        &self,
        session_id: &str,
        direction: &str,
        message: &RendezvousMessage,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            INSERT INTO rendezvous_messages 
                (session_id, direction, message_type, content, created_ts)
            VALUES ($1, $2, $3, $4, $5)
            "#,
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

    pub async fn get_messages(
        &self,
        session_id: &str,
        after_id: Option<i64>,
    ) -> Result<Vec<StoredRendezvousMessage>, sqlx::Error> {
        match after_id {
            Some(after) => {
                sqlx::query_as::<_, StoredRendezvousMessage>(
                    r#"
                    SELECT * FROM rendezvous_messages 
                    WHERE session_id = $1 AND id > $2
                    ORDER BY id ASC
                    "#,
                )
                .bind(session_id)
                .bind(after)
                .fetch_all(&*self.pool)
                .await
            }
            None => {
                sqlx::query_as::<_, StoredRendezvousMessage>(
                    r#"
                    SELECT * FROM rendezvous_messages 
                    WHERE session_id = $1
                    ORDER BY id ASC
                    "#,
                )
                .bind(session_id)
                .fetch_all(&*self.pool)
                .await
            }
        }
    }

    pub async fn delete_messages(&self, session_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM rendezvous_messages WHERE session_id = $1
            "#,
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
        assert_eq!(
            RendezvousIntent::LoginReciprocate.as_str(),
            "login.reciprocate"
        );
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

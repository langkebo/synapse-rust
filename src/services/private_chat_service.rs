use crate::services::*;
use serde_json::json;
use std::sync::Arc;

/// Storage service for private chat sessions and messages.
/// Handles direct messaging between users with encryption support.
#[derive(Clone)]
pub struct PrivateChatStorage {
    pool: Arc<sqlx::PgPool>,
}

impl PrivateChatStorage {
    /// Creates a new PrivateChatStorage instance.
    ///
    /// # Arguments
    /// * `pool` - Shared PostgreSQL connection pool
    ///
    /// # Returns
    /// A new PrivateChatStorage instance
    pub fn new(pool: &Arc<sqlx::PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// Creates the required tables for private messaging if they don't exist.
    ///
    /// Creates the following tables:
    /// - private_sessions: Stores chat sessions between user pairs
    /// - private_messages: Stores individual messages in sessions
    /// - session_keys: Stores encryption keys for sessions
    ///
    /// # Returns
    /// Result indicating success or database error
    ///
    /// # Errors
    /// Returns sqlx::Error if table creation fails
    pub async fn create_tables(&self) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS private_sessions (
                id SERIAL PRIMARY KEY,
                session_id VARCHAR(255) NOT NULL UNIQUE,
                user_id_1 VARCHAR(255) NOT NULL,
                user_id_2 VARCHAR(255) NOT NULL,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL,
                last_message_ts BIGINT,
                unread_count INT DEFAULT 0
            )
            "#
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS private_messages (
                id SERIAL PRIMARY KEY,
                session_id VARCHAR(255) NOT NULL,
                sender_id VARCHAR(255) NOT NULL,
                message_type VARCHAR(50) NOT NULL,
                content TEXT NOT NULL,
                encrypted_content TEXT,
                read_by_receiver BOOLEAN DEFAULT FALSE,
                created_ts BIGINT NOT NULL
            )
            "#
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS session_keys (
                id SERIAL PRIMARY KEY,
                session_id VARCHAR(255) NOT NULL,
                sender_id VARCHAR(255) NOT NULL,
                key_data TEXT NOT NULL,
                created_ts BIGINT NOT NULL
            )
            "#
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// Creates a new private chat session between two users.
    ///
    /// # Arguments
    /// * `user_id_1` - First user's ID
    /// * `user_id_2` - Second user's ID
    ///
    /// # Returns
    /// Result containing the new session ID
    ///
    /// # Errors
    /// Returns sqlx::Error if database operation fails
    pub async fn create_session(
        &self,
        user_id_1: &str,
        user_id_2: &str,
    ) -> Result<String, sqlx::Error> {
        let session_id = format!("ps_{}", uuid::Uuid::new_v4().to_string().replace("-", ""));
        let now = chrono::Utc::now().timestamp();

        sqlx::query!(
            r#"
            INSERT INTO private_sessions (id, user_id_1, user_id_2, session_type, created_ts, last_activity_ts)
            VALUES ($1, $2, $3, 'direct', $4, $4)
            "#,
            session_id,
            user_id_1,
            user_id_2,
            now
        )
        .execute(&*self.pool)
        .await?;

        Ok(session_id)
    }

    /// Gets an existing session between two users or creates a new one.
    ///
    /// # Arguments
    /// * `user_id_1` - First user's ID
    /// * `user_id_2` - Second user's ID
    ///
    /// # Returns
    /// Result containing the session ID
    ///
    /// # Errors
    /// Returns sqlx::Error if database operation fails
    pub async fn get_or_create_session(
        &self,
        user_id_1: &str,
        user_id_2: &str,
    ) -> Result<String, sqlx::Error> {
        let existing = sqlx::query!(
            r#"
            SELECT id FROM private_sessions
            WHERE (user_id_1 = $1 AND user_id_2 = $2) OR (user_id_1 = $2 AND user_id_2 = $1)
            "#,
            user_id_1,
            user_id_2
        )
        .fetch_optional(&*self.pool)
        .await?;

        if let Some(row) = existing {
            return Ok(row.id);
        }

        self.create_session(user_id_1, user_id_2).await
    }

    /// Gets all private chat sessions for a user.
    ///
    /// # Arguments
    /// * `user_id` - The user's ID
    ///
    /// # Returns
    /// Result containing a vector of SessionInfo sorted by last activity
    ///
    /// # Errors
    /// Returns sqlx::Error if database operation fails
    pub async fn get_user_sessions(&self, user_id: &str) -> Result<Vec<SessionInfo>, sqlx::Error> {
        let rows: Vec<(String, String, String, i64, i64, i64, i32)> = sqlx::query_as(
            r#"
            SELECT id, user_id_1, user_id_2, created_ts, last_activity_ts, updated_ts, COALESCE(unread_count, 0) as unread_count
            FROM private_sessions
            WHERE user_id_1 = $1 OR user_id_2 = $1
            ORDER BY last_activity_ts DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool).await?;
        Ok(rows
            .iter()
            .map(|r| SessionInfo {
                session_id: r.0.clone(),
                other_user: if r.1 == user_id {
                    r.2.clone()
                } else {
                    r.1.clone()
                },
                created_ts: r.3,
                updated_ts: r.5,
                last_message_ts: Some(r.4),
                unread_count: r.6,
            })
            .collect())
    }

    /// Sends a message in a private chat session.
    ///
    /// # Arguments
    /// * `session_id` - The private session ID
    /// * `sender_id` - The sender's user ID
    /// * `message_type` - Type of message (e.g., "m.text")
    /// * `content` - The message content
    /// * `encrypted_content` - Optional encrypted content for E2EE
    ///
    /// # Returns
    /// Result containing the new message ID
    ///
    /// # Errors
    /// Returns sqlx::Error if database operation fails
    pub async fn send_message(
        &self,
        session_id: &str,
        sender_id: &str,
        message_type: &str,
        content: &str,
        encrypted_content: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        let result = sqlx::query!(
            r#"
            INSERT INTO private_messages (session_id, sender_id, message_type, content, encrypted_content, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#,
            session_id, sender_id, message_type, content, encrypted_content, now
        ).fetch_one(&*self.pool).await?;

        sqlx::query(
            r#"
            UPDATE private_sessions SET updated_ts = $1, last_message_ts = $1 WHERE id = $2
            "#,
        )
        .bind(now)
        .bind(session_id)
        .execute(&*self.pool)
        .await?;

        Ok(result.id)
    }

    /// Gets messages from a private session with pagination.
    ///
    /// # Arguments
    /// * `session_id` - The private session ID
    /// * `limit` - Maximum number of messages to return
    /// * `before` - Optional timestamp to get messages before
    ///
    /// # Returns
    /// Result containing a vector of MessageInfo
    ///
    /// # Errors
    /// Returns sqlx::Error if database operation fails
    pub async fn get_session_messages(
        &self,
        session_id: &str,
        limit: i64,
        before: Option<i64>,
    ) -> Result<Vec<MessageInfo>, sqlx::Error> {
        let query = if let Some(ts) = before {
            let rows: Vec<PrivateMessageRow> = sqlx::query_as(
                r#"
                SELECT id, session_id, sender_id, message_type, content, encrypted_content, read_by_receiver, created_ts
                FROM private_messages
                WHERE session_id = $1 AND created_ts < $2
                ORDER BY created_ts DESC
                LIMIT $3
                "#,
            )
            .bind(session_id)
            .bind(ts)
            .bind(limit)
            .fetch_all(&*self.pool).await?;
            rows
        } else {
            let rows: Vec<PrivateMessageRow> = sqlx::query_as(
                r#"
                SELECT id, session_id, sender_id, message_type, content, encrypted_content, read_by_receiver, created_ts
                FROM private_messages
                WHERE session_id = $1
                ORDER BY created_ts DESC
                LIMIT $2
                "#,
            )
            .bind(session_id)
            .bind(limit)
            .fetch_all(&*self.pool).await?;
            rows
        };

        Ok(query
            .iter()
            .map(|r| MessageInfo {
                id: r.0,
                session_id: r.1.clone(),
                sender_id: r.2.clone(),
                message_type: r.3.clone(),
                content: r.4.clone(),
                encrypted_content: r.5.clone(),
                read_by_receiver: r.6,
                created_ts: r.7,
            })
            .collect())
    }

    /// Marks all messages in a session as read by the user.
    ///
    /// # Arguments
    /// * `session_id` - The private session ID
    /// * `user_id` - The user's ID (receiver)
    ///
    /// # Returns
    /// Result indicating success
    ///
    /// # Errors
    /// Returns sqlx::Error if database operation fails
    pub async fn mark_as_read(&self, session_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE private_messages SET read_by_receiver = TRUE
            WHERE session_id = $1 AND sender_id != $2 AND read_by_receiver = FALSE
            "#,
        )
        .bind(session_id)
        .bind(user_id)
        .execute(&*self.pool)
        .await?;

        sqlx::query(r#"UPDATE private_sessions SET unread_count = 0 WHERE id = $1"#)
            .bind(session_id)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    /// Gets the total unread message count for a user across all sessions.
    ///
    /// # Arguments
    /// * `user_id` - The user's ID
    ///
    /// # Returns
    /// Result containing the total unread count
    ///
    /// # Errors
    /// Returns sqlx::Error if database operation fails
    pub async fn get_unread_count(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT COALESCE(SUM(unread_count), 0) as total_unread
            FROM private_sessions
            WHERE user_id_1 = $1 OR user_id_2 = $1
            "#,
            user_id
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(result.total_unread.unwrap_or(0) as i64)
    }

    pub async fn search_messages(
        &self,
        user_id: &str,
        query: &str,
        limit: i64,
    ) -> Result<Vec<SearchResult>, sqlx::Error> {
        let search_pattern = format!("%{}%", query);
        let rows: Vec<(i64, String, String, String, String, i64, String)> = sqlx::query_as(
            r#"
            SELECT m.id, m.session_id, m.sender_id, m.message_type, m.content, m.created_ts,
                   CASE WHEN s.user_id_1 = $1 THEN s.user_id_2 ELSE s.user_id_1 END as other_user
            FROM private_messages m
            JOIN private_sessions s ON m.session_id = s.session_id
            WHERE (s.user_id_1 = $1 OR s.user_id_2 = $1)
            AND (m.content ILIKE $2 OR m.encrypted_content ILIKE $2)
            ORDER BY m.created_ts DESC
            LIMIT $3
            "#,
        )
        .bind(user_id)
        .bind(search_pattern)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| SearchResult {
                message_id: r.0,
                session_id: r.1.clone(),
                sender_id: r.2.clone(),
                message_type: r.3.clone(),
                content: r.4.clone(),
                other_user: r.6.clone(),
                created_ts: r.5,
            })
            .collect())
    }

    pub async fn delete_session(&self, session_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        let session: Option<(String, String)> =
            sqlx::query_as(r#"SELECT user_id_1, user_id_2 FROM private_sessions WHERE id = $1"#)
                .bind(session_id)
                .fetch_optional(&*self.pool)
                .await?;

        if let Some(s) = session {
            if s.0 == user_id || s.1 == user_id {
                sqlx::query(r#"DELETE FROM private_messages WHERE session_id = $1"#)
                    .bind(session_id)
                    .execute(&*self.pool)
                    .await?;
                sqlx::query(r#"DELETE FROM session_keys WHERE session_id = $1"#)
                    .bind(session_id)
                    .execute(&*self.pool)
                    .await?;
                sqlx::query(r#"DELETE FROM private_sessions WHERE id = $1"#)
                    .bind(session_id)
                    .execute(&*self.pool)
                    .await?;
            }
        }
        Ok(())
    }

    pub async fn delete_message(&self, message_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM private_messages
            WHERE id = $1
            "#,
        )
        .bind(message_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct SessionInfo {
    pub session_id: String,
    pub other_user: String,
    pub created_ts: i64,
    pub updated_ts: i64,
    pub last_message_ts: Option<i64>,
    pub unread_count: i32,
}

#[derive(Debug)]
pub struct MessageInfo {
    pub id: i64,
    pub session_id: String,
    pub sender_id: String,
    pub message_type: String,
    pub content: String,
    pub encrypted_content: Option<String>,
    pub read_by_receiver: bool,
    pub created_ts: i64,
}

type PrivateMessageRow = (
    i64,
    String,
    String,
    String,
    String,
    Option<String>,
    bool,
    i64,
);

#[derive(Debug)]
pub struct SearchResult {
    pub message_id: i64,
    pub session_id: String,
    pub sender_id: String,
    pub message_type: String,
    pub content: String,
    pub other_user: String,
    pub created_ts: i64,
}

pub struct PrivateChatService<'a> {
    services: &'a ServiceContainer,
    chat_storage: PrivateChatStorage,
    search_service: Arc<crate::services::search_service::SearchService>,
}

impl<'a> PrivateChatService<'a> {
    pub fn new(
        services: &'a ServiceContainer,
        pool: &Arc<sqlx::PgPool>,
        search_service: Arc<crate::services::search_service::SearchService>,
    ) -> Self {
        Self {
            services,
            chat_storage: PrivateChatStorage::new(pool),
            search_service,
        }
    }

    pub async fn get_or_create_session(
        &self,
        user_id: &str,
        other_user_id: &str,
    ) -> ApiResult<serde_json::Value> {
        if user_id == other_user_id {
            return Err(ApiError::bad_request(
                "Cannot create session with yourself".to_string(),
            ));
        }

        let friend_storage = FriendStorage::new(&self.services.user_storage.pool);
        let is_friend = friend_storage
            .is_friend(user_id, other_user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if !is_friend {
            let share_room = self
                .services
                .member_storage
                .share_common_room(user_id, other_user_id)
                .await
                .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

            if !share_room {
                return Err(ApiError::forbidden(
                    "Cannot send private messages to non-friends. You must be friends or share a common room.".to_string(),
                ));
            }
        }

        let session_id = self
            .chat_storage
            .get_or_create_session(user_id, other_user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let registration_service = RegistrationService::new(self.services);
        let other_profile = registration_service
            .get_profile(other_user_id)
            .await
            .unwrap_or(json!({ "user_id": other_user_id }));

        Ok(json!({
            "session_id": session_id,
            "other_user": other_profile,
            "created": chrono::Utc::now().to_rfc3339()
        }))
    }

    pub async fn get_sessions(&self, user_id: &str) -> ApiResult<serde_json::Value> {
        let sessions = self
            .chat_storage
            .get_user_sessions(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let mut session_list = Vec::new();
        let registration_service = RegistrationService::new(self.services);

        for session in sessions {
            let profile = registration_service
                .get_profile(&session.other_user)
                .await
                .unwrap_or(json!({ "user_id": session.other_user }));

            let last_message = self
                .chat_storage
                .get_session_messages(&session.session_id, 1, None)
                .await
                .ok()
                .and_then(|mut msgs| msgs.pop())
                .map(|m| {
                    json!({
                        "content": m.content,
                        "sender_id": m.sender_id,
                        "created_ts": m.created_ts
                    })
                });

            session_list.push(json!({
                "session_id": session.session_id,
                "other_user": profile,
                "created_ts": session.created_ts,
                "updated_ts": session.updated_ts,
                "unread_count": session.unread_count,
                "last_message": last_message
            }));
        }

        Ok(json!({
            "sessions": session_list,
            "count": session_list.len()
        }))
    }

    pub async fn send_message(
        &self,
        user_id: &str,
        session_id: &str,
        message_type: &str,
        content: &serde_json::Value,
        encrypted: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        let content_str = content.to_string();
        let message_id = self
            .chat_storage
            .send_message(session_id, user_id, message_type, &content_str, encrypted)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let created_ts = chrono::Utc::now().timestamp();

        // Dual-write to Elasticsearch
        if self.search_service.is_enabled() {
            let _ = self
                .search_service
                .index_message(message_id, session_id, user_id, &content_str, created_ts)
                .await;
        }

        Ok(json!({
            "message_id": format!("pm_{}", message_id),
            "session_id": session_id,
            "created_ts": created_ts * 1000
        }))
    }

    pub async fn get_messages(
        &self,
        _user_id: &str,
        session_id: &str,
        limit: i64,
        before: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        let before_ts = before.and_then(|s| s.parse().ok());
        let messages = self
            .chat_storage
            .get_session_messages(session_id, limit, before_ts)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let message_list: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| {
                json!({
                    "message_id": format!("pm_{}", m.id),
                    "sender_id": m.sender_id,
                    "message_type": m.message_type,
                    "content": serde_json::from_str(&m.content).unwrap_or(json!({})),
                    "encrypted_content": m.encrypted_content,
                    "read_by_receiver": m.read_by_receiver,
                    "created_ts": m.created_ts
                })
            })
            .collect();

        Ok(json!({
            "messages": message_list,
            "count": message_list.len()
        }))
    }

    pub async fn mark_session_read(&self, user_id: &str, session_id: &str) -> ApiResult<()> {
        self.chat_storage
            .mark_as_read(session_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
        Ok(())
    }

    pub async fn search_messages(
        &self,
        user_id: &str,
        query: &str,
        limit: i64,
    ) -> ApiResult<serde_json::Value> {
        // Dynamic Routing: Use ES if enabled, fallback to PG
        if self.search_service.is_enabled() {
            match self
                .search_service
                .search_messages(user_id, query, limit)
                .await
            {
                Ok(results) => {
                    return Ok(json!({
                        "results": results,
                        "count": results.len(),
                        "query": query
                    }))
                }
                Err(e) => {
                    ::tracing::warn!("Elasticsearch search failed, falling back to PG: {}", e);
                }
            }
        }

        let results = self
            .chat_storage
            .search_messages(user_id, query, limit)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let result_list: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                json!({
                    "message_id": format!("pm_{}", r.message_id),
                    "session_id": r.session_id,
                    "sender_id": r.sender_id,
                    "message_type": r.message_type,
                    "content": r.content,
                    "other_user": r.other_user,
                    "created_ts": r.created_ts
                })
            })
            .collect();

        Ok(json!({
            "results": result_list,
            "count": result_list.len(),
            "query": query
        }))
    }

    pub async fn delete_session(&self, user_id: &str, session_id: &str) -> ApiResult<()> {
        self.chat_storage
            .delete_session(session_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
        Ok(())
    }

    pub async fn get_unread_count(&self, user_id: &str) -> ApiResult<i64> {
        self.chat_storage
            .get_unread_count(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))
    }

    pub async fn delete_message(&self, user_id: &str, message_id: &str) -> ApiResult<()> {
        let message_id_parsed = message_id
            .parse::<i64>()
            .map_err(|_| ApiError::bad_request("Invalid message ID".to_string()))?;

        let message: Option<(i64, String, String)> = sqlx::query_as(
            r#"SELECT id, sender_id, session_id FROM private_messages WHERE id = $1"#,
        )
        .bind(message_id_parsed)
        .fetch_optional(&*self.chat_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let message =
            message.ok_or_else(|| ApiError::not_found("Message not found".to_string()))?;

        if message.1 != user_id {
            return Err(ApiError::forbidden(
                "You can only delete your own messages".to_string(),
            ));
        }

        self.chat_storage
            .delete_message(message_id_parsed)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::{Pool, Postgres, Row};
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    async fn setup_test_database() -> Pool<Postgres> {
        let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://synapse:synapse@localhost:5432/synapse_test".to_string()
        });

        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS users (
                user_id TEXT PRIMARY KEY,
                username TEXT NOT NULL UNIQUE,
                password_hash TEXT,
                creation_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create users table");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS private_sessions (
                id VARCHAR(255) PRIMARY KEY,
                user_id TEXT NOT NULL,
                other_user_id TEXT NOT NULL,
                session_type VARCHAR(50) DEFAULT 'direct',
                created_ts BIGINT NOT NULL,
                last_activity_ts BIGINT NOT NULL,
                updated_ts BIGINT,
                unread_count INT DEFAULT 0
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create private_sessions table");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS private_messages (
                id BIGSERIAL PRIMARY KEY,
                session_id VARCHAR(255) NOT NULL,
                sender_id TEXT NOT NULL,
                message_type VARCHAR(50) DEFAULT 'text',
                content TEXT,
                encrypted_content TEXT,
                read_by_receiver BOOLEAN DEFAULT FALSE,
                created_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create private_messages table");

        pool
    }

    async fn cleanup_test_database(pool: &Pool<Postgres>) {
        sqlx::query("DROP TABLE IF EXISTS private_messages CASCADE")
            .execute(pool)
            .await
            .ok();

        sqlx::query("DROP TABLE IF EXISTS private_sessions CASCADE")
            .execute(pool)
            .await
            .ok();

        sqlx::query("DROP TABLE IF EXISTS users CASCADE")
            .execute(pool)
            .await
            .ok();
    }

    async fn create_test_user(pool: &Pool<Postgres>, user_id: &str, username: &str) {
        sqlx::query(
            r#"
            INSERT INTO users (user_id, username, creation_ts)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(user_id)
        .bind(username)
        .bind(chrono::Utc::now().timestamp())
        .execute(pool)
        .await
        .expect("Failed to create test user");
    }

    async fn create_test_session(
        pool: &Pool<Postgres>,
        session_id: &str,
        user_id: &str,
        other_user_id: &str,
    ) {
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            r#"
            INSERT INTO private_sessions (id, user_id, other_user_id, created_ts, last_activity_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(session_id)
        .bind(user_id)
        .bind(other_user_id)
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .expect("Failed to create test session");
    }

    async fn create_test_message(
        pool: &Pool<Postgres>,
        session_id: &str,
        sender_id: &str,
        content: &str,
    ) -> i64 {
        let result = sqlx::query(
            r#"
            INSERT INTO private_messages (session_id, sender_id, content, created_ts)
            VALUES ($1, $2, $3, $4)
            RETURNING id
            "#,
        )
        .bind(session_id)
        .bind(sender_id)
        .bind(content)
        .bind(chrono::Utc::now().timestamp())
        .fetch_one(pool)
        .await
        .expect("Failed to create test message");

        result
            .try_get::<i64, _>("id")
            .expect("Failed to get message id")
    }

    #[test]
    fn test_delete_message_success() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = setup_test_database().await;

            create_test_user(&pool, "@alice:example.com", "alice").await;
            create_test_user(&pool, "@bob:example.com", "bob").await;
            create_test_session(&pool, "session_1", "@alice:example.com", "@bob:example.com").await;

            let message_id =
                create_test_message(&pool, "session_1", "@alice:example.com", "Hello Bob!").await;

            let chat_storage = PrivateChatStorage::new(&Arc::new(pool.clone()));
            let result = chat_storage.delete_message(message_id).await;

            assert!(result.is_ok(), "Failed to delete message");

            let message_exists = sqlx::query_as::<_, (bool,)>(
                "SELECT EXISTS(SELECT 1 FROM private_messages WHERE id = $1)",
            )
            .bind(message_id)
            .fetch_one(&pool)
            .await
            .expect("Failed to check message existence");

            assert!(!message_exists.0, "Message still exists after deletion");

            cleanup_test_database(&pool).await;
        });
    }

    #[test]
    fn test_delete_message_nonexistent() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = setup_test_database().await;

            create_test_user(&pool, "@alice:example.com", "alice").await;

            let chat_storage = PrivateChatStorage::new(&Arc::new(pool.clone()));
            let result = chat_storage.delete_message(999999).await;

            assert!(
                result.is_ok(),
                "Deleting non-existent message should succeed"
            );

            cleanup_test_database(&pool).await;
        });
    }

    #[test]
    fn test_delete_message_service_authorization_success() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = setup_test_database().await;

            create_test_user(&pool, "@alice:example.com", "alice").await;
            create_test_user(&pool, "@bob:example.com", "bob").await;
            create_test_session(&pool, "session_1", "@alice:example.com", "@bob:example.com").await;

            let message_id =
                create_test_message(&pool, "session_1", "@alice:example.com", "Hello Bob!").await;

            let services = ServiceContainer::new_test();
            let chat_service = PrivateChatService::new(
                &services,
                &Arc::new(pool.clone()),
                services.search_service.clone(),
            );

            let result = chat_service
                .delete_message("@alice:example.com", &message_id.to_string())
                .await;

            assert!(
                result.is_ok(),
                "User should be able to delete their own message"
            );

            let message_exists = sqlx::query_as::<_, (bool,)>(
                "SELECT EXISTS(SELECT 1 FROM private_messages WHERE id = $1)",
            )
            .bind(message_id)
            .fetch_one(&pool)
            .await
            .expect("Failed to check message existence");

            assert!(!message_exists.0, "Message should be deleted");

            cleanup_test_database(&pool).await;
        });
    }

    #[test]
    fn test_delete_message_service_authorization_failure() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = setup_test_database().await;

            create_test_user(&pool, "@alice:example.com", "alice").await;
            create_test_user(&pool, "@bob:example.com", "bob").await;
            create_test_session(&pool, "session_1", "@alice:example.com", "@bob:example.com").await;

            let message_id =
                create_test_message(&pool, "session_1", "@alice:example.com", "Hello Bob!").await;

            let services = ServiceContainer::new_test();
            let chat_service = PrivateChatService::new(
                &services,
                &Arc::new(pool.clone()),
                services.search_service.clone(),
            );

            let result = chat_service
                .delete_message("@bob:example.com", &message_id.to_string())
                .await;

            assert!(
                result.is_err(),
                "User should not be able to delete others' message"
            );

            match result {
                Err(e) => {
                    assert_eq!(e.code(), "M_FORBIDDEN", "Should return forbidden status");
                }
                _ => panic!("Expected error"),
            }

            let message_exists = sqlx::query_as::<_, (bool,)>(
                "SELECT EXISTS(SELECT 1 FROM private_messages WHERE id = $1)",
            )
            .bind(message_id)
            .fetch_one(&pool)
            .await
            .expect("Failed to check message existence");

            assert!(message_exists.0, "Message should still exist");

            cleanup_test_database(&pool).await;
        });
    }

    #[test]
    fn test_delete_message_invalid_id() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = setup_test_database().await;

            create_test_user(&pool, "@alice:example.com", "alice").await;

            let services = ServiceContainer::new_test();
            let chat_service = PrivateChatService::new(
                &services,
                &Arc::new(pool.clone()),
                services.search_service.clone(),
            );

            let result = chat_service
                .delete_message("@alice:example.com", "invalid_id")
                .await;

            assert!(
                result.is_err(),
                "Should return error for invalid message ID"
            );

            match result {
                Err(e) => {
                    assert_eq!(e.code(), "M_BAD_JSON", "Should return bad request status");
                }
                _ => panic!("Expected error"),
            }

            cleanup_test_database(&pool).await;
        });
    }

    #[test]
    fn test_delete_message_not_found() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = setup_test_database().await;

            create_test_user(&pool, "@alice:example.com", "alice").await;

            let services = ServiceContainer::new_test();
            let chat_service = PrivateChatService::new(
                &services,
                &Arc::new(pool.clone()),
                services.search_service.clone(),
            );

            let result = chat_service
                .delete_message("@alice:example.com", "999999")
                .await;

            assert!(
                result.is_err(),
                "Should return error for non-existent message"
            );

            match result {
                Err(e) => {
                    assert_eq!(e.code(), "M_NOT_FOUND", "Should return not found status");
                }
                _ => panic!("Expected error"),
            }

            cleanup_test_database(&pool).await;
        });
    }

    #[test]
    fn test_delete_multiple_messages() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = setup_test_database().await;

            create_test_user(&pool, "@alice:example.com", "alice").await;
            create_test_user(&pool, "@bob:example.com", "bob").await;
            create_test_session(&pool, "session_1", "@alice:example.com", "@bob:example.com").await;

            let message_id_1 =
                create_test_message(&pool, "session_1", "@alice:example.com", "Message 1").await;

            let message_id_2 =
                create_test_message(&pool, "session_1", "@alice:example.com", "Message 2").await;

            let message_id_3 =
                create_test_message(&pool, "session_1", "@alice:example.com", "Message 3").await;

            let chat_storage = PrivateChatStorage::new(&Arc::new(pool.clone()));

            chat_storage.delete_message(message_id_1).await.unwrap();
            chat_storage.delete_message(message_id_2).await.unwrap();
            chat_storage.delete_message(message_id_3).await.unwrap();

            let message_count: (i64,) =
                sqlx::query_as("SELECT COUNT(*) FROM private_messages WHERE session_id = $1")
                    .bind("session_1")
                    .fetch_one(&pool)
                    .await
                    .expect("Failed to count messages");

            assert_eq!(message_count.0, 0, "All messages should be deleted");

            cleanup_test_database(&pool).await;
        });
    }
}

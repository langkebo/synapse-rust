use crate::services::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

/// Storage service for private chat sessions and messages.
/// Handles direct messaging between users with encryption support.
#[derive(Clone)]
pub struct PrivateChatStorage {
    pool: Arc<sqlx::PgPool>,
}

impl PrivateChatStorage {
    pub fn new(pool: &Arc<sqlx::PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn get_session_details(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionDetails>, sqlx::Error> {
        sqlx::query_as::<_, SessionDetails>(
            r#"
            SELECT id as session_id, user_id_1, user_id_2, created_ts, updated_ts, last_activity_ts, unread_count
            FROM private_sessions
            WHERE id = $1
            "#,
        )
        .bind(session_id)
        .fetch_optional(&*self.pool)
        .await
    }

    /// Creates the required tables for private messaging if they don't exist.
    pub async fn create_tables(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS private_sessions (
                id VARCHAR(255) PRIMARY KEY,
                user_id_1 VARCHAR(255) NOT NULL,
                user_id_2 VARCHAR(255) NOT NULL,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL,
                last_activity_ts BIGINT,
                unread_count INT DEFAULT 0
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS private_messages (
                id BIGSERIAL PRIMARY KEY,
                session_id VARCHAR(255) NOT NULL,
                sender_id VARCHAR(255) NOT NULL,
                message_type VARCHAR(50) NOT NULL,
                content TEXT NOT NULL,
                encrypted_content TEXT,
                read_by_receiver BOOLEAN DEFAULT FALSE,
                created_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS session_keys (
                id BIGSERIAL PRIMARY KEY,
                session_id VARCHAR(255) NOT NULL,
                sender_id VARCHAR(255) NOT NULL,
                key_data TEXT NOT NULL,
                created_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn create_session(
        &self,
        user_id_1: &str,
        user_id_2: &str,
    ) -> Result<String, sqlx::Error> {
        let session_id = format!("ps_{}", uuid::Uuid::new_v4().to_string().replace("-", ""));
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            r#"
            INSERT INTO private_sessions (id, user_id_1, user_id_2, created_ts, updated_ts, last_activity_ts)
            VALUES ($1, $2, $3, $4, $4, $4)
            "#
        )
        .bind(&session_id)
        .bind(user_id_1)
        .bind(user_id_2)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(session_id)
    }

    pub async fn get_or_create_session(
        &self,
        user_id_1: &str,
        user_id_2: &str,
    ) -> Result<String, sqlx::Error> {
        let existing: Option<(String,)> = sqlx::query_as(
            r#"
            SELECT id FROM private_sessions
            WHERE (user_id_1 = $1 AND user_id_2 = $2) OR (user_id_1 = $2 AND user_id_2 = $1)
            "#,
        )
        .bind(user_id_1)
        .bind(user_id_2)
        .fetch_optional(&*self.pool)
        .await?;

        if let Some(row) = existing {
            return Ok(row.0);
        }

        self.create_session(user_id_1, user_id_2).await
    }

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
                last_activity_ts: Some(r.4),
                unread_count: r.6,
            })
            .collect())
    }

    pub async fn send_message(
        &self,
        session_id: &str,
        sender_id: &str,
        message_type: &str,
        content: &str,
        encrypted_content: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        let row: (i64,) = sqlx::query_as(
            r#"
            INSERT INTO private_messages (session_id, sender_id, message_type, content, encrypted_content, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#
        )
        .bind(session_id)
        .bind(sender_id)
        .bind(message_type)
        .bind(content)
        .bind(encrypted_content)
        .bind(now)
        .fetch_one(&*self.pool).await?;

        sqlx::query(
            r#"
            UPDATE private_sessions SET updated_ts = $1, last_activity_ts = $1 WHERE id = $2
            "#,
        )
        .bind(now)
        .bind(session_id)
        .execute(&*self.pool)
        .await?;

        Ok(row.0)
    }

    pub async fn get_session_messages(
        &self,
        session_id: &str,
        limit: i64,
        before: Option<i64>,
    ) -> Result<Vec<MessageInfo>, sqlx::Error> {
        let rows: Vec<PrivateMessageRow> = if let Some(ts) = before {
            sqlx::query_as(
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
            .fetch_all(&*self.pool).await?
        } else {
            sqlx::query_as(
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
            .fetch_all(&*self.pool).await?
        };

        Ok(rows
            .into_iter()
            .map(|r| MessageInfo {
                id: r.0,
                session_id: r.1,
                sender_id: r.2,
                message_type: r.3,
                content: r.4,
                encrypted_content: r.5,
                read_by_receiver: r.6,
                created_ts: r.7,
            })
            .collect())
    }

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

    pub async fn get_unread_count(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        let result: (Option<i64>,) = sqlx::query_as(
            r#"
            SELECT COALESCE(SUM(unread_count), 0) as total_unread
            FROM private_sessions
            WHERE user_id_1 = $1 OR user_id_2 = $1
            "#,
        )
        .bind(user_id)
        .fetch_one(&*self.pool)
        .await?;

        Ok(result.0.unwrap_or(0))
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
            JOIN private_sessions s ON m.session_id = s.id
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
            .into_iter()
            .map(|r| SearchResult {
                message_id: r.0,
                session_id: r.1,
                sender_id: r.2,
                message_type: r.3,
                content: r.4,
                other_user: r.6,
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

    pub async fn get_session_info(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionInfo>, sqlx::Error> {
        let row: Option<(String, String, String, i64, i64, i64, i32)> = sqlx::query_as(
            r#"
            SELECT id, user_id_1, user_id_2, created_ts, last_activity_ts, updated_ts, COALESCE(unread_count, 0) as unread_count
            FROM private_sessions
            WHERE id = $1
            "#,
        )
        .bind(session_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(|r| SessionInfo {
            session_id: r.0,
            other_user: r.1,
            created_ts: r.3,
            updated_ts: r.5,
            last_activity_ts: Some(r.4),
            unread_count: r.6,
        }))
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct SessionDetails {
    pub session_id: String,
    pub user_id_1: String,
    pub user_id_2: String,
    pub created_ts: i64,
    pub updated_ts: i64,
    pub last_activity_ts: Option<i64>,
    pub unread_count: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionInfo {
    pub session_id: String,
    pub other_user: String,
    pub created_ts: i64,
    pub updated_ts: i64,
    pub last_activity_ts: Option<i64>,
    pub unread_count: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SearchResult {
    pub message_id: i64,
    pub session_id: String,
    pub sender_id: String,
    pub message_type: String,
    pub content: String,
    pub other_user: String,
    pub created_ts: i64,
}

pub struct PrivateChatService {
    pool: Arc<sqlx::PgPool>,
    member_storage: RoomMemberStorage,
    chat_storage: PrivateChatStorage,
    search_service: Arc<crate::services::search_service::SearchService>,
}

impl PrivateChatService {
    pub fn new(
        pool: &Arc<sqlx::PgPool>,
        search_service: Arc<crate::services::search_service::SearchService>,
        server_name: String,
    ) -> Self {
        let _ = server_name;
        Self {
            pool: pool.clone(),
            member_storage: RoomMemberStorage::new(pool),
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

        let friend_storage = FriendStorage::new(&self.pool);
        let is_friend = friend_storage
            .is_friend(user_id, other_user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if !is_friend {
            let share_room = self
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

        Ok(json!({
            "session_id": session_id,
            "other_user": other_user_id,
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
        for session in sessions {
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
                "other_user": session.other_user,
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

    pub async fn get_session_details(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> ApiResult<serde_json::Value> {
        let session = self
            .chat_storage
            .get_session_details(session_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Session not found".to_string()))?;

        if session.user_id_1 != user_id && session.user_id_2 != user_id {
            return Err(ApiError::forbidden(
                "You are not a participant of this session".to_string(),
            ));
        }

        let other_user_id = if session.user_id_1 == user_id {
            session.user_id_2
        } else {
            session.user_id_1
        };

        let messages = self.get_messages(user_id, session_id, 50, None).await?;

        Ok(json!({
            "session_id": session.session_id,
            "other_user": other_user_id,
            "created_ts": session.created_ts,
            "updated_ts": session.updated_ts,
            "last_activity_ts": session.last_activity_ts,
            "unread_count": session.unread_count,
            "messages": messages["messages"]
        }))
    }
}

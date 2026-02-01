use sqlx::{Pool, Postgres, Row};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct PrivateSession {
    pub id: String,
    pub user_id_1: String,
    pub user_id_2: String,
    pub session_type: String,
    pub encryption_key: Option<String>,
    pub created_ts: i64,
    pub last_activity_ts: i64,
    pub updated_ts: Option<i64>,
    pub unread_count: Option<i32>,
    pub encrypted_content: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PrivateMessage {
    pub id: i64,
    pub session_id: String,
    pub sender_id: String,
    pub message_type: String,
    pub content: Option<String>,
    pub encrypted_content: Option<String>,
    pub read_by_receiver: bool,
    pub created_ts: i64,
}

#[derive(Debug, Clone)]
pub struct CreatePrivateSession {
    pub id: String,
    pub user_id_1: String,
    pub user_id_2: String,
    pub session_type: String,
    pub encryption_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CreatePrivateMessage {
    pub session_id: String,
    pub sender_id: String,
    pub message_type: String,
    pub content: Option<String>,
    pub encrypted_content: Option<String>,
}

pub struct PrivateSessionStorage {
    pool: Arc<Pool<Postgres>>,
}

impl PrivateSessionStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_session(
        &self,
        session: &CreatePrivateSession,
    ) -> Result<PrivateSession, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let row = sqlx::query(
            r#"
            INSERT INTO private_sessions (id, user_id_1, user_id_2, session_type, encryption_key, created_ts, last_activity_ts, updated_ts, unread_count)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 0)
            RETURNING *
            "#,
        )
        .bind(&session.id)
        .bind(&session.user_id_1)
        .bind(&session.user_id_2)
        .bind(&session.session_type)
        .bind(&session.encryption_key)
        .bind(now)
        .bind(now)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(PrivateSession {
            id: row.get("id"),
            user_id_1: row.get("user_id_1"),
            user_id_2: row.get("user_id_2"),
            session_type: row.get("session_type"),
            encryption_key: row.get("encryption_key"),
            created_ts: row.get("created_ts"),
            last_activity_ts: row.get("last_activity_ts"),
            updated_ts: row.get("updated_ts"),
            unread_count: row.get("unread_count"),
            encrypted_content: row.get("encrypted_content"),
        })
    }

    pub async fn get_session(
        &self,
        session_id: &str,
    ) -> Result<Option<PrivateSession>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT * FROM private_sessions
            WHERE id = $1
            "#,
        )
        .bind(session_id)
        .fetch_optional(&*self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(PrivateSession {
                id: row.get("id"),
                user_id_1: row.get("user_id_1"),
                user_id_2: row.get("user_id_2"),
                session_type: row.get("session_type"),
                encryption_key: row.get("encryption_key"),
                created_ts: row.get("created_ts"),
                last_activity_ts: row.get("last_activity_ts"),
                updated_ts: row.get("updated_ts"),
                unread_count: row.get("unread_count"),
                encrypted_content: row.get("encrypted_content"),
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn get_user_sessions(
        &self,
        user_id: &str,
    ) -> Result<Vec<PrivateSession>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT * FROM private_sessions
            WHERE user_id_1 = $1 OR user_id_2 = $1
            ORDER BY last_activity_ts DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;

        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(PrivateSession {
                id: row.get("id"),
                user_id_1: row.get("user_id_1"),
                user_id_2: row.get("user_id_2"),
                session_type: row.get("session_type"),
                encryption_key: row.get("encryption_key"),
                created_ts: row.get("created_ts"),
                last_activity_ts: row.get("last_activity_ts"),
                updated_ts: row.get("updated_ts"),
                unread_count: row.get("unread_count"),
                encrypted_content: row.get("encrypted_content"),
            });
        }

        Ok(sessions)
    }

    pub async fn get_session_by_users(
        &self,
        user_id: &str,
        other_user_id: &str,
    ) -> Result<Option<PrivateSession>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT * FROM private_sessions
            WHERE (user_id_1 = $1 AND user_id_2 = $2) OR (user_id_1 = $2 AND user_id_2 = $1)
            "#,
        )
        .bind(user_id)
        .bind(other_user_id)
        .fetch_optional(&*self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(PrivateSession {
                id: row.get("id"),
                user_id_1: row.get("user_id_1"),
                user_id_2: row.get("user_id_2"),
                session_type: row.get("session_type"),
                encryption_key: row.get("encryption_key"),
                created_ts: row.get("created_ts"),
                last_activity_ts: row.get("last_activity_ts"),
                updated_ts: row.get("updated_ts"),
                unread_count: row.get("unread_count"),
                encrypted_content: row.get("encrypted_content"),
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn update_session_activity(&self, session_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE private_sessions
            SET last_activity_ts = $1, updated_ts = $1
            WHERE id = $2
            "#,
        )
        .bind(now)
        .bind(session_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn increment_unread_count(&self, session_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE private_sessions
            SET unread_count = unread_count + 1
            WHERE id = $1
            "#,
        )
        .bind(session_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn reset_unread_count(&self, session_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE private_sessions
            SET unread_count = 0
            WHERE id = $1
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
            DELETE FROM private_sessions
            WHERE id = $1
            "#,
        )
        .bind(session_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}

pub struct PrivateMessageStorage {
    pool: Arc<Pool<Postgres>>,
}

impl PrivateMessageStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_message(
        &self,
        message: &CreatePrivateMessage,
    ) -> Result<PrivateMessage, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let row = sqlx::query(
            r#"
            INSERT INTO private_messages (session_id, sender_id, message_type, content, encrypted_content, read_by_receiver, created_ts)
            VALUES ($1, $2, $3, $4, $5, false, $6)
            RETURNING *
            "#,
        )
        .bind(&message.session_id)
        .bind(&message.sender_id)
        .bind(&message.message_type)
        .bind(&message.content)
        .bind(&message.encrypted_content)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(PrivateMessage {
            id: row.get("id"),
            session_id: row.get("session_id"),
            sender_id: row.get("sender_id"),
            message_type: row.get("message_type"),
            content: row.get("content"),
            encrypted_content: row.get("encrypted_content"),
            read_by_receiver: row.get("read_by_receiver"),
            created_ts: row.get("created_ts"),
        })
    }

    pub async fn get_messages(
        &self,
        session_id: &str,
        limit: i64,
        before: Option<i64>,
    ) -> Result<Vec<PrivateMessage>, sqlx::Error> {
        let rows = if let Some(before_ts) = before {
            sqlx::query(
                r#"
                SELECT * FROM private_messages
                WHERE session_id = $1 AND created_ts < $2
                ORDER BY created_ts DESC
                LIMIT $3
                "#,
            )
            .bind(session_id)
            .bind(before_ts)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT * FROM private_messages
                WHERE session_id = $1
                ORDER BY created_ts DESC
                LIMIT $2
                "#,
            )
            .bind(session_id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await?
        };

        let mut messages = Vec::new();
        for row in rows {
            messages.push(PrivateMessage {
                id: row.get("id"),
                session_id: row.get("session_id"),
                sender_id: row.get("sender_id"),
                message_type: row.get("message_type"),
                content: row.get("content"),
                encrypted_content: row.get("encrypted_content"),
                read_by_receiver: row.get("read_by_receiver"),
                created_ts: row.get("created_ts"),
            });
        }

        Ok(messages)
    }

    pub async fn mark_as_read(&self, session_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE private_messages
            SET read_by_receiver = true
            WHERE session_id = $1 AND sender_id != $2 AND read_by_receiver = false
            "#,
        )
        .bind(session_id)
        .bind(user_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn search_messages(
        &self,
        user_id: &str,
        query: &str,
        limit: i64,
    ) -> Result<Vec<PrivateMessage>, sqlx::Error> {
        let search_pattern = format!("%{}%", query);
        let rows = sqlx::query(
            r#"
            SELECT pm.* FROM private_messages pm
            INNER JOIN private_sessions ps ON pm.session_id = ps.id
            WHERE (ps.user_id_1 = $1 OR ps.user_id_2 = $1)
            AND (pm.content ILIKE $2 OR pm.encrypted_content ILIKE $2)
            ORDER BY pm.created_ts DESC
            LIMIT $3
            "#,
        )
        .bind(user_id)
        .bind(&search_pattern)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(PrivateMessage {
                id: row.get("id"),
                session_id: row.get("session_id"),
                sender_id: row.get("sender_id"),
                message_type: row.get("message_type"),
                content: row.get("content"),
                encrypted_content: row.get("encrypted_content"),
                read_by_receiver: row.get("read_by_receiver"),
                created_ts: row.get("created_ts"),
            });
        }

        Ok(messages)
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

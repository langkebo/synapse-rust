use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Clone)]
pub struct PrivateChatStorage {
    pool: Arc<Pool<Postgres>>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct PrivateSession {
    pub session_id: String,
    pub user_id_1: String,
    pub user_id_2: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_message_id: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct PrivateMessage {
    pub message_id: String,
    pub session_id: String,
    pub sender_id: String,
    pub content: serde_json::Value,
    pub created_at: i64,
    pub is_read: bool,
}

impl PrivateChatStorage {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    /// 获取或创建私聊会话
    /// 
    /// 确保 user1 和 user2 之间的会话唯一。
    /// 始终按字典序排序 user_id 以保证唯一性。
    pub async fn get_or_create_session(&self, user1: &str, user2: &str) -> Result<PrivateSession, sqlx::Error> {
        let (u1, u2) = if user1 < user2 { (user1, user2) } else { (user2, user1) };
        let now = chrono::Utc::now().timestamp_millis();

        // 尝试获取现有会话
        let session = sqlx::query_as::<_, PrivateSession>(
            r#"
            SELECT session_id, user_id_1, user_id_2, created_at, updated_at, last_message_id
            FROM private_sessions
            WHERE user_id_1 = $1 AND user_id_2 = $2
            "#
        )
        .bind(u1)
        .bind(u2)
        .fetch_optional(&*self.pool)
        .await?;

        if let Some(s) = session {
            return Ok(s);
        }

        // 创建新会话
        let session_id = format!("ps_{}", uuid::Uuid::new_v4());
        let new_session = sqlx::query_as::<_, PrivateSession>(
            r#"
            INSERT INTO private_sessions (session_id, user_id_1, user_id_2, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $4)
            RETURNING session_id, user_id_1, user_id_2, created_at, updated_at, last_message_id
            "#
        )
        .bind(&session_id)
        .bind(u1)
        .bind(u2)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(new_session)
    }

    /// 获取用户的会话列表
    pub async fn get_user_sessions(&self, user_id: &str) -> Result<Vec<PrivateSession>, sqlx::Error> {
        sqlx::query_as::<_, PrivateSession>(
            r#"
            SELECT session_id, user_id_1, user_id_2, created_at, updated_at, last_message_id
            FROM private_sessions
            WHERE user_id_1 = $1 OR user_id_2 = $1
            ORDER BY updated_at DESC
            "#
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
    }

    /// 保存消息
    pub async fn save_message(
        &self, 
        session_id: &str, 
        sender_id: &str, 
        content: serde_json::Value
    ) -> Result<PrivateMessage, sqlx::Error> {
        let message_id = format!("pm_{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().timestamp_millis();

        let mut tx = self.pool.begin().await?;

        // 插入消息
        let message = sqlx::query_as::<_, PrivateMessage>(
            r#"
            INSERT INTO private_messages (message_id, session_id, sender_id, content, created_at, is_read)
            VALUES ($1, $2, $3, $4, $5, false)
            RETURNING message_id, session_id, sender_id, content, created_at, is_read
            "#
        )
        .bind(&message_id)
        .bind(session_id)
        .bind(sender_id)
        .bind(content)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;

        // 更新会话的 last_message_id 和 updated_at
        sqlx::query(
            r#"
            UPDATE private_sessions
            SET last_message_id = $1, updated_at = $2
            WHERE session_id = $3
            "#
        )
        .bind(&message_id)
        .bind(now)
        .bind(session_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(message)
    }

    /// 删除会话及其所有消息
    /// 返回删除的消息数量
    pub async fn delete_session(&self, user_id: &str, session_id: &str) -> Result<u64, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        // 验证会话属于该用户
        let session_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM private_sessions WHERE session_id = $1 AND (user_id_1 = $2 OR user_id_2 = $2))"
        )
        .bind(session_id)
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await?;

        if !session_exists {
            return Err(sqlx::Error::RowNotFound);
        }

        // 先删除会话的所有消息
        let delete_count = sqlx::query(
            "DELETE FROM private_messages WHERE session_id = $1"
        )
        .bind(session_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        // 删除会话
        sqlx::query(
            "DELETE FROM private_sessions WHERE session_id = $1"
        )
        .bind(session_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(delete_count)
    }

    /// 获取会话历史消息
    pub async fn get_messages(
        &self,
        session_id: &str,
        limit: i64,
        before_id: Option<String>
    ) -> Result<Vec<PrivateMessage>, sqlx::Error> {
        if let Some(before) = before_id {
            // 获取 before_id 的时间戳
            let before_ts: i64 = sqlx::query_scalar(
                "SELECT created_at FROM private_messages WHERE message_id = $1"
            )
            .bind(before)
            .fetch_one(&*self.pool)
            .await?;

            sqlx::query_as::<_, PrivateMessage>(
                r#"
                SELECT message_id, session_id, sender_id, content, created_at, is_read
                FROM private_messages
                WHERE session_id = $1 AND created_at < $2
                ORDER BY created_at DESC
                LIMIT $3
                "#
            )
            .bind(session_id)
            .bind(before_ts)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        } else {
            sqlx::query_as::<_, PrivateMessage>(
                r#"
                SELECT message_id, session_id, sender_id, content, created_at, is_read
                FROM private_messages
                WHERE session_id = $1
                ORDER BY created_at DESC
                LIMIT $2
                "#
            )
            .bind(session_id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        }
    }
}

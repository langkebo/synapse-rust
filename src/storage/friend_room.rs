use sqlx::{Pool, Postgres, Row};
use std::sync::Arc;

#[derive(Clone)]
pub struct FriendRoomStorage {
    pool: Arc<Pool<Postgres>>,
}

impl FriendRoomStorage {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    /// 查找用户的好友列表房间 ID
    pub async fn get_friend_list_room_id(&self, user_id: &str) -> Result<Option<String>, sqlx::Error> {
        // 使用 runtime query 避免 SQLX_OFFLINE 问题
        let row = sqlx::query(
            r#"
            SELECT e.room_id
            FROM events e
            JOIN state_events se ON e.event_id = se.event_id
            WHERE e.type = 'm.room.create'
            AND e.sender = $1
            AND (e.content->>'type') = 'm.friends'
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(|r| r.get("room_id")))
    }

    /// 获取房间内的所有好友列表事件内容
    pub async fn get_friend_list_content(&self, room_id: &str) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT e.content
            FROM current_state_events cse
            JOIN events e ON cse.event_id = e.event_id
            WHERE cse.room_id = $1
            AND cse.type = 'm.friends.list'
            AND cse.state_key = ''
            LIMIT 1
            "#,
        )
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(|r| r.get("content")))
    }
}

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
        let row = sqlx::query(
            r#"
            SELECT e.room_id
            FROM events e
            JOIN rooms r ON e.room_id = r.room_id
            WHERE e.event_type = 'm.room.create'
            AND e.sender = $1
            AND e.content->>'type' = 'm.friends'
            ORDER BY e.origin_server_ts DESC
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
            FROM events e
            WHERE e.room_id = $1
            AND e.event_type = 'm.friends.list'
            AND e.state_key = ''
            ORDER BY e.origin_server_ts DESC
            LIMIT 1
            "#,
        )
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(|r| r.get("content")))
    }

    /// 获取好友请求列表
    pub async fn get_friend_requests(
        &self,
        room_id: &str,
        request_type: &str,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let event_type = format!("m.friend_requests.{}", request_type);
        
        let row = sqlx::query(
            r#"
            SELECT e.content
            FROM events e
            WHERE e.room_id = $1
            AND e.event_type = $2
            AND e.state_key = ''
            ORDER BY e.origin_server_ts DESC
            LIMIT 1
            "#,
        )
        .bind(room_id)
        .bind(&event_type)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row
            .and_then(|r| r.get::<Option<serde_json::Value>, _>("content"))
            .and_then(|c| c.get("requests").cloned())
            .and_then(|r| r.as_array().cloned())
            .unwrap_or_default())
    }

    /// 检查用户是否在好友列表中
    pub async fn is_friend(&self, room_id: &str, friend_id: &str) -> Result<bool, sqlx::Error> {
        let content = self.get_friend_list_content(room_id).await?;
        
        Ok(content
            .and_then(|c| c.get("friends").cloned())
            .and_then(|f| f.as_array().cloned())
            .map(|friends| {
                friends.iter().any(|f| {
                    f.get("user_id")
                        .and_then(|u| u.as_str())
                        .map(|u| u == friend_id)
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false))
    }

    /// 获取好友信息
    pub async fn get_friend_info(
        &self,
        room_id: &str,
        friend_id: &str,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let content = self.get_friend_list_content(room_id).await?;
        
        Ok(content
            .and_then(|c| c.get("friends").cloned())
            .and_then(|f| f.as_array().cloned())
            .and_then(|friends| {
                friends.iter().find(|f| {
                    f.get("user_id")
                        .and_then(|u| u.as_str())
                        .map(|u| u == friend_id)
                        .unwrap_or(false)
                }).cloned()
            }))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_storage_creation() {
    }
}

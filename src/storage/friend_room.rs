use sqlx::{Pool, Postgres, Row};
use std::sync::Arc;

/// 创建好友分组的参数
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateFriendGroupParams {
    pub room_id: String,
    pub user_id: String,
    pub group_name: String,
}

/// 添加好友到分组的参数
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AddFriendToGroupParams {
    pub room_id: String,
    pub user_id: String,
    pub group_name: String,
    pub friend_id: String,
}

/// 从分组移除好友的参数
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RemoveFriendFromGroupParams {
    pub room_id: String,
    pub user_id: String,
    pub group_name: String,
    pub friend_id: String,
}

/// 重命名好友分组的参数
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RenameFriendGroupParams {
    pub room_id: String,
    pub user_id: String,
    pub old_group_name: String,
    pub new_group_name: String,
}

#[derive(Clone)]
pub struct FriendRoomStorage {
    pool: Arc<Pool<Postgres>>,
}

impl FriendRoomStorage {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    /// 查找用户的好友列表房间 ID
    pub async fn get_friend_list_room_id(
        &self,
        user_id: &str,
    ) -> Result<Option<String>, sqlx::Error> {
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
    pub async fn get_friend_list_content(
        &self,
        room_id: &str,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
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
                friends
                    .iter()
                    .find(|f| {
                        f.get("user_id")
                            .and_then(|u| u.as_str())
                            .map(|u| u == friend_id)
                            .unwrap_or(false)
                    })
                    .cloned()
            }))
    }

    /// 获取好友分组信息
    pub async fn get_friend_groups(
        &self,
        room_id: &str,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT e.content
            FROM events e
            WHERE e.room_id = $1
            AND e.event_type = 'm.friends.groups'
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

    /// 获取好友所在的分组列表
    pub async fn get_friend_groups_for_user(
        &self,
        room_id: &str,
        friend_id: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        let groups = self.get_friend_groups(room_id).await?;

        Ok(groups
            .and_then(|g| g.get("groups").cloned())
            .and_then(|g| g.as_array().cloned())
            .map(|groups| {
                groups
                    .iter()
                    .filter_map(|g| {
                        let members = g.get("members")?.as_array()?;
                        let group_name = g.get("name")?.as_str()?;
                        if members.iter().any(|m| m.as_str() == Some(friend_id)) {
                            Some(group_name.to_string())
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// 创建好友分组
    pub async fn create_friend_group(
        &self,
        room_id: &str,
        user_id: &str,
        group_name: &str,
    ) -> Result<(), sqlx::Error> {
        let mut groups = self.get_friend_groups(room_id).await?;
        let now = chrono::Utc::now().timestamp_millis();

        if groups.is_none() {
            groups = Some(serde_json::json!({
                "groups": [],
                "version": 1,
                "updated_ts": now
            }));
        }

        let groups_val = groups.unwrap_or(serde_json::json!({"groups": []}));
        let mut groups_array = groups_val
            .get("groups")
            .and_then(|g| g.as_array().cloned())
            .unwrap_or_default();

        let exists = groups_array.iter().any(|g| {
            g.get("name")
                .and_then(|n| n.as_str())
                .map(|n| n == group_name)
                .unwrap_or(false)
        });

        if exists {
            return Err(sqlx::Error::RowNotFound);
        }

        groups_array.push(serde_json::json!({
            "name": group_name,
            "members": [],
            "created_ts": now,
            "updated_ts": now
        }));

        let updated_groups = serde_json::json!({
            "groups": groups_array,
            "version": groups_val.get("version").and_then(|v| v.as_i64()).unwrap_or(1) + 1,
            "updated_ts": now
        });

        self.save_friend_groups(room_id, user_id, &updated_groups).await
    }

    /// 删除好友分组
    pub async fn delete_friend_group(
        &self,
        room_id: &str,
        user_id: &str,
        group_name: &str,
    ) -> Result<bool, sqlx::Error> {
        let groups = self.get_friend_groups(room_id).await?;
        let now = chrono::Utc::now().timestamp_millis();

        let groups_val = groups.unwrap_or(serde_json::json!({"groups": []}));
        let groups_array = groups_val
            .get("groups")
            .and_then(|g| g.as_array().cloned())
            .unwrap_or_default();

        let filtered: Vec<_> = groups_array
            .iter()
            .filter(|g| {
                g.get("name")
                    .and_then(|n| n.as_str())
                    .map(|n| n != group_name)
                    .unwrap_or(true)
            })
            .cloned()
            .collect();

        if filtered.len() == groups_array.len() {
            return Ok(false);
        }

        let updated_groups = serde_json::json!({
            "groups": filtered,
            "version": groups_val.get("version").and_then(|v| v.as_i64()).unwrap_or(1) + 1,
            "updated_ts": now
        });

        self.save_friend_groups(room_id, user_id, &updated_groups).await?;
        Ok(true)
    }

    /// 重命名好友分组
    pub async fn rename_friend_group(
        &self,
        room_id: &str,
        user_id: &str,
        old_name: &str,
        new_name: &str,
    ) -> Result<bool, sqlx::Error> {
        let groups = self.get_friend_groups(room_id).await?;
        let now = chrono::Utc::now().timestamp_millis();

        let groups_val = groups.unwrap_or(serde_json::json!({"groups": []}));
        let mut groups_array = groups_val
            .get("groups")
            .and_then(|g| g.as_array().cloned())
            .unwrap_or_default();

        let mut found = false;
        for group in &mut groups_array {
            if group.get("name").and_then(|n| n.as_str()) == Some(old_name) {
                group["name"] = serde_json::json!(new_name);
                group["updated_ts"] = serde_json::json!(now);
                found = true;
                break;
            }
        }

        if !found {
            return Ok(false);
        }

        let updated_groups = serde_json::json!({
            "groups": groups_array,
            "version": groups_val.get("version").and_then(|v| v.as_i64()).unwrap_or(1) + 1,
            "updated_ts": now
        });

        self.save_friend_groups(room_id, user_id, &updated_groups).await?;
        Ok(true)
    }

    /// 添加好友到分组
    pub async fn add_friend_to_group(
        &self,
        room_id: &str,
        user_id: &str,
        group_name: &str,
        friend_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let groups = self.get_friend_groups(room_id).await?;
        let now = chrono::Utc::now().timestamp_millis();

        let groups_val = groups.unwrap_or(serde_json::json!({"groups": []}));
        let mut groups_array = groups_val
            .get("groups")
            .and_then(|g| g.as_array().cloned())
            .unwrap_or_default();

        let mut found = false;
        for group in &mut groups_array {
            if group.get("name").and_then(|n| n.as_str()) == Some(group_name) {
                let members = group
                    .get("members")
                    .and_then(|m| m.as_array().cloned())
                    .unwrap_or_default();

                if members.iter().any(|m| m.as_str() == Some(friend_id)) {
                    return Ok(false);
                }

                let mut updated_members = members;
                updated_members.push(serde_json::json!(friend_id));
                group["members"] = serde_json::json!(updated_members);
                group["updated_ts"] = serde_json::json!(now);
                found = true;
                break;
            }
        }

        if !found {
            return Ok(false);
        }

        let updated_groups = serde_json::json!({
            "groups": groups_array,
            "version": groups_val.get("version").and_then(|v| v.as_i64()).unwrap_or(1) + 1,
            "updated_ts": now
        });

        self.save_friend_groups(room_id, user_id, &updated_groups).await?;
        Ok(true)
    }

    /// 从分组移除好友
    pub async fn remove_friend_from_group(
        &self,
        room_id: &str,
        user_id: &str,
        group_name: &str,
        friend_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let groups = self.get_friend_groups(room_id).await?;
        let now = chrono::Utc::now().timestamp_millis();

        let groups_val = groups.unwrap_or(serde_json::json!({"groups": []}));
        let mut groups_array = groups_val
            .get("groups")
            .and_then(|g| g.as_array().cloned())
            .unwrap_or_default();

        let mut found = false;
        for group in &mut groups_array {
            if group.get("name").and_then(|n| n.as_str()) == Some(group_name) {
                let members = group
                    .get("members")
                    .and_then(|m| m.as_array().cloned())
                    .unwrap_or_default();

                let filtered: Vec<_> = members
                    .iter()
                    .filter(|m| m.as_str() != Some(friend_id))
                    .cloned()
                    .collect();

                if filtered.len() == members.len() {
                    return Ok(false);
                }

                group["members"] = serde_json::json!(filtered);
                group["updated_ts"] = serde_json::json!(now);
                found = true;
                break;
            }
        }

        if !found {
            return Ok(false);
        }

        let updated_groups = serde_json::json!({
            "groups": groups_array,
            "version": groups_val.get("version").and_then(|v| v.as_i64()).unwrap_or(1) + 1,
            "updated_ts": now
        });

        self.save_friend_groups(room_id, user_id, &updated_groups).await?;
        Ok(true)
    }

    /// 保存好友分组数据
    async fn save_friend_groups(
        &self,
        room_id: &str,
        user_id: &str,
        content: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let event_id = format!("${}", uuid::Uuid::new_v4().simple());

        sqlx::query(
            r#"
            INSERT INTO events (event_id, room_id, sender, event_type, state_key, content, origin_server_ts, depth)
            VALUES ($1, $2, $3, 'm.friends.groups', '', $4, $5, 1)
            "#,
        )
        .bind(&event_id)
        .bind(room_id)
        .bind(user_id)
        .bind(content)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_storage_creation() {}
}

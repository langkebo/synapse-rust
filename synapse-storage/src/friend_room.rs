use async_trait::async_trait;
use sqlx::{Pool, Postgres, Row};
use std::sync::Arc;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FriendDmLink {
    pub owner_user_id: String,
    pub friend_room_id: String,
    pub content: serde_json::Value,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DirectRoomFallbackLink {
    pub other_user_id: String,
    pub room_id: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DmPartnerRecord {
    pub user_id: String,
    pub display_name: String,
    pub avatar_url: String,
}

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

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct FriendRequestRecord {
    pub id: i64,
    pub sender_id: String,
    pub receiver_id: String,
    pub message: Option<String>,
    pub status: String,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

#[async_trait]
pub trait FriendRoomStoreApi: Send + Sync {
    async fn get_friend_list_room_id(&self, user_id: &str) -> Result<Option<String>, sqlx::Error>;
    async fn get_friend_list_content(&self, room_id: &str) -> Result<Option<serde_json::Value>, sqlx::Error>;
    async fn find_friend_lists_by_dm_room_id(&self, dm_room_id: &str) -> Result<Vec<FriendDmLink>, sqlx::Error>;
    async fn get_effective_direct_links_fallback(
        &self,
        user_id: &str,
    ) -> Result<Vec<DirectRoomFallbackLink>, sqlx::Error>;
    async fn get_existing_direct_room_id(&self, user_id: &str, friend_id: &str) -> Result<Option<String>, sqlx::Error>;
    async fn get_dm_partner_for_room(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<Option<DmPartnerRecord>, sqlx::Error>;
    async fn get_friend_requests(
        &self,
        room_id: &str,
        request_type: &str,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error>;
    async fn is_friend(&self, room_id: &str, friend_id: &str) -> Result<bool, sqlx::Error>;
    async fn get_friend_info(&self, room_id: &str, friend_id: &str) -> Result<Option<serde_json::Value>, sqlx::Error>;
    async fn get_friend_groups(&self, room_id: &str) -> Result<Option<serde_json::Value>, sqlx::Error>;
    async fn get_friend_groups_for_user(&self, room_id: &str, friend_id: &str) -> Result<Vec<String>, sqlx::Error>;
    async fn create_friend_group(&self, room_id: &str, user_id: &str, group_name: &str) -> Result<(), sqlx::Error>;
    async fn delete_friend_group(&self, room_id: &str, user_id: &str, group_name: &str) -> Result<bool, sqlx::Error>;
    async fn rename_friend_group(
        &self,
        room_id: &str,
        user_id: &str,
        old_name: &str,
        new_name: &str,
    ) -> Result<bool, sqlx::Error>;
    async fn add_friend_to_group(
        &self,
        room_id: &str,
        user_id: &str,
        group_name: &str,
        friend_id: &str,
    ) -> Result<bool, sqlx::Error>;
    async fn remove_friend_from_group(
        &self,
        room_id: &str,
        user_id: &str,
        group_name: &str,
        friend_id: &str,
    ) -> Result<bool, sqlx::Error>;
    async fn create_friend_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
        message: Option<&str>,
    ) -> Result<i64, sqlx::Error>;
    async fn get_friend_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
    ) -> Result<Option<FriendRequestRecord>, sqlx::Error>;
    async fn get_pending_friend_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
    ) -> Result<Option<FriendRequestRecord>, sqlx::Error>;
    async fn get_incoming_friend_requests(&self, receiver_id: &str) -> Result<Vec<FriendRequestRecord>, sqlx::Error>;
    async fn get_outgoing_friend_requests(&self, sender_id: &str) -> Result<Vec<FriendRequestRecord>, sqlx::Error>;
    async fn update_friend_request_status(
        &self,
        sender_id: &str,
        receiver_id: &str,
        status: &str,
    ) -> Result<bool, sqlx::Error>;
    async fn delete_friend_request(&self, sender_id: &str, receiver_id: &str) -> Result<bool, sqlx::Error>;
    async fn has_pending_request(&self, sender_id: &str, receiver_id: &str) -> Result<bool, sqlx::Error>;
    async fn has_any_pending_request(&self, user_a: &str, user_b: &str) -> Result<bool, sqlx::Error>;
    async fn ensure_user_exists(&self, user_id: &str) -> Result<(), sqlx::Error>;
    async fn create_friend_request_with_user_ensure(
        &self,
        sender_id: &str,
        receiver_id: &str,
        message: Option<&str>,
    ) -> Result<i64, sqlx::Error>;
    async fn get_mutual_friends(&self, user_id: &str, target_user_id: &str) -> Result<Vec<String>, sqlx::Error>;
    async fn get_user_friend_ids(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error>;
    async fn get_shared_rooms(&self, user_id: &str, target_user_id: &str) -> Result<Vec<String>, sqlx::Error>;
    async fn get_friend_suggestions_from_mutual_friends(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error>;
    async fn get_friend_suggestions_from_shared_rooms(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error>;
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
    pub async fn get_friend_list_room_id(&self, user_id: &str) -> Result<Option<String>, sqlx::Error> {
        let row = sqlx::query(
            r"
            SELECT e.room_id
            FROM events e
            JOIN rooms r ON e.room_id = r.room_id
            WHERE e.event_type = 'm.room.create'
            AND e.sender = $1
            AND e.content->>'type' = 'm.friends'
            ORDER BY e.origin_server_ts DESC
            LIMIT 1
            ",
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(|r| r.get("room_id")))
    }

    /// 获取房间内的所有好友列表事件内容
    pub async fn get_friend_list_content(&self, room_id: &str) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let row = sqlx::query(
            r"
            SELECT e.content
            FROM events e
            WHERE e.room_id = $1
            AND e.event_type = 'm.friends.list'
            AND e.state_key = ''
            ORDER BY e.origin_server_ts DESC
            LIMIT 1
            ",
        )
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(|r| r.get("content")))
    }

    /// 根据好友 DM 房间 ID 反查所有关联的好友列表快照。
    pub async fn find_friend_lists_by_dm_room_id(&self, dm_room_id: &str) -> Result<Vec<FriendDmLink>, sqlx::Error> {
        sqlx::query_as::<_, FriendDmLink>(
            r"
            WITH latest_friend_lists AS (
                SELECT DISTINCT ON (COALESCE(sender, user_id))
                    COALESCE(sender, user_id) AS owner_user_id,
                    room_id AS friend_room_id,
                    content,
                    origin_server_ts
                FROM events
                WHERE event_type = 'm.friends.list'
                  AND state_key = ''
                ORDER BY COALESCE(sender, user_id), origin_server_ts DESC
            )
            SELECT owner_user_id, friend_room_id, content
            FROM latest_friend_lists
            WHERE EXISTS (
                SELECT 1
                FROM jsonb_array_elements(COALESCE(content->'friends', '[]'::jsonb)) AS friend
                WHERE friend->>'dm_room_id' = $1
            )
            ",
        )
        .bind(dm_room_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_effective_direct_links_fallback(
        &self,
        user_id: &str,
    ) -> Result<Vec<DirectRoomFallbackLink>, sqlx::Error> {
        sqlx::query_as::<_, DirectRoomFallbackLink>(
            r"
            SELECT rm_other.user_id AS other_user_id, rm_user.room_id
            FROM room_memberships rm_user
            JOIN room_summaries rs
              ON rs.room_id = rm_user.room_id
             AND rs.is_direct = TRUE
            JOIN room_memberships rm_other
              ON rm_other.room_id = rm_user.room_id
             AND rm_other.user_id <> $1
             AND rm_other.membership IN ('join', 'invite')
            WHERE rm_user.user_id = $1
              AND rm_user.membership IN ('join', 'invite')
              AND (
                SELECT COUNT(*)
                FROM room_memberships rm_count
                WHERE rm_count.room_id = rm_user.room_id
                  AND rm_count.membership IN ('join', 'invite')
              ) = 2
            ",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_existing_direct_room_id(
        &self,
        user_id: &str,
        friend_id: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        let row = sqlx::query(
            r"
            SELECT m1.room_id
            FROM room_memberships m1
            JOIN room_memberships m2 ON m1.room_id = m2.room_id
            JOIN room_summaries rs ON m1.room_id = rs.room_id
            WHERE m1.user_id = $1
              AND m2.user_id = $2
              AND m1.membership IN ('join', 'invite')
              AND m2.membership IN ('join', 'invite')
              AND rs.is_direct = true
            LIMIT 1
            ",
        )
        .bind(user_id)
        .bind(friend_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(|value| value.get::<String, _>("room_id")))
    }

    pub async fn get_dm_partner_for_room(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<Option<DmPartnerRecord>, sqlx::Error> {
        sqlx::query_as::<_, DmPartnerRecord>(
            r"
            SELECT
                rm.user_id,
                COALESCE(rm.display_name, u.displayname, u.username, '') AS display_name,
                COALESCE(rm.avatar_url, u.avatar_url, '') AS avatar_url
            FROM room_memberships rm
            LEFT JOIN users u ON u.user_id = rm.user_id
            WHERE rm.room_id = $1
              AND rm.user_id <> $2
              AND rm.membership IN ('join', 'invite')
            ORDER BY CASE WHEN rm.membership = 'join' THEN 0 ELSE 1 END, rm.updated_ts DESC NULLS LAST
            LIMIT 1
            ",
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await
    }

    /// 获取好友请求列表
    pub async fn get_friend_requests(
        &self,
        room_id: &str,
        request_type: &str,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let event_type = format!("m.friend_requests.{request_type}");

        let row = sqlx::query(
            r"
            SELECT e.content
            FROM events e
            WHERE e.room_id = $1
            AND e.event_type = $2
            AND e.state_key = ''
            ORDER BY e.origin_server_ts DESC
            LIMIT 1
            ",
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

        Ok(content.and_then(|c| c.get("friends").cloned()).and_then(|f| f.as_array().cloned()).is_some_and(|friends| {
            friends.iter().any(|f| f.get("user_id").and_then(|u| u.as_str()).is_some_and(|u| u == friend_id))
        }))
    }

    /// 获取好友信息
    pub async fn get_friend_info(
        &self,
        room_id: &str,
        friend_id: &str,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let content = self.get_friend_list_content(room_id).await?;

        Ok(content.and_then(|c| c.get("friends").cloned()).and_then(|f| f.as_array().cloned()).and_then(|friends| {
            friends.iter().find(|f| f.get("user_id").and_then(|u| u.as_str()).is_some_and(|u| u == friend_id)).cloned()
        }))
    }

    /// 获取好友分组信息
    pub async fn get_friend_groups(&self, room_id: &str) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let row = sqlx::query(
            r"
            SELECT e.content
            FROM events e
            WHERE e.room_id = $1
            AND e.event_type = 'm.friends.groups'
            AND e.state_key = ''
            ORDER BY e.origin_server_ts DESC
            LIMIT 1
            ",
        )
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(|r| r.get("content")))
    }

    /// 获取好友所在的分组列表
    pub async fn get_friend_groups_for_user(&self, room_id: &str, friend_id: &str) -> Result<Vec<String>, sqlx::Error> {
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
    pub async fn create_friend_group(&self, room_id: &str, user_id: &str, group_name: &str) -> Result<(), sqlx::Error> {
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
        let mut groups_array = groups_val.get("groups").and_then(|g| g.as_array().cloned()).unwrap_or_default();

        let exists =
            groups_array.iter().any(|g| g.get("name").and_then(|n| n.as_str()).is_some_and(|n| n == group_name));

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
        let groups_array = groups_val.get("groups").and_then(|g| g.as_array().cloned()).unwrap_or_default();

        let filtered: Vec<_> = groups_array
            .iter()
            .filter(|g| g.get("name").and_then(|n| n.as_str()) != Some(group_name))
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
        let mut groups_array = groups_val.get("groups").and_then(|g| g.as_array().cloned()).unwrap_or_default();

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
        let mut groups_array = groups_val.get("groups").and_then(|g| g.as_array().cloned()).unwrap_or_default();

        let mut found = false;
        for group in &mut groups_array {
            if group.get("name").and_then(|n| n.as_str()) == Some(group_name) {
                let members = group.get("members").and_then(|m| m.as_array().cloned()).unwrap_or_default();

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
        let mut groups_array = groups_val.get("groups").and_then(|g| g.as_array().cloned()).unwrap_or_default();

        let mut found = false;
        for group in &mut groups_array {
            if group.get("name").and_then(|n| n.as_str()) == Some(group_name) {
                let members = group.get("members").and_then(|m| m.as_array().cloned()).unwrap_or_default();

                let filtered: Vec<_> = members.iter().filter(|m| m.as_str() != Some(friend_id)).cloned().collect();

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
            r"
            INSERT INTO events (event_id, room_id, sender, event_type, state_key, content, origin_server_ts, depth)
            VALUES ($1, $2, $3, 'm.friends.groups', '', $4, $5, 1)
            ",
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

    // ========================================================================
    // 好友请求数据库操作 (使用 friend_requests 表)
    // ========================================================================

    pub async fn create_friend_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
        message: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let row = sqlx::query(
            r"
            INSERT INTO friend_requests (sender_id, receiver_id, message, status, created_ts)
            VALUES ($1, $2, $3, 'pending', $4)
            ON CONFLICT (sender_id, receiver_id)
            DO UPDATE SET status = 'pending', updated_ts = $4, message = $3
            RETURNING id
            ",
        )
        .bind(sender_id)
        .bind(receiver_id)
        .bind(message)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await?;

        if let Some(row) = row {
            return Ok(row.get("id"));
        }

        let fallback = sqlx::query("SELECT id FROM friend_requests WHERE sender_id = $1 AND receiver_id = $2")
            .bind(sender_id)
            .bind(receiver_id)
            .fetch_optional(&*self.pool)
            .await?;

        match fallback {
            Some(row) => Ok(row.get("id")),
            None => {
                tracing::error!(
                    "INSERT...ON CONFLICT RETURNING produced no row and no existing record for sender={} receiver={}",
                    sender_id,
                    receiver_id
                );
                Err(sqlx::Error::RowNotFound)
            }
        }
    }

    pub async fn get_friend_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
    ) -> Result<Option<FriendRequestRecord>, sqlx::Error> {
        let row = sqlx::query_as::<_, FriendRequestRecord>(
            r"
            SELECT id, sender_id, receiver_id, message, status, created_ts, updated_ts
            FROM friend_requests
            WHERE sender_id = $1 AND receiver_id = $2
            ",
        )
        .bind(sender_id)
        .bind(receiver_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_pending_friend_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
    ) -> Result<Option<FriendRequestRecord>, sqlx::Error> {
        let row = sqlx::query_as::<_, FriendRequestRecord>(
            r"
            SELECT id, sender_id, receiver_id, message, status, created_ts, updated_ts
            FROM friend_requests
            WHERE sender_id = $1 AND receiver_id = $2 AND status = 'pending'
            ",
        )
        .bind(sender_id)
        .bind(receiver_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_incoming_friend_requests(
        &self,
        receiver_id: &str,
    ) -> Result<Vec<FriendRequestRecord>, sqlx::Error> {
        let rows = sqlx::query_as::<_, FriendRequestRecord>(
            r"
            SELECT id, sender_id, receiver_id, message, status, created_ts, updated_ts
            FROM friend_requests
            WHERE receiver_id = $1 AND status = 'pending'
            ORDER BY created_ts DESC
            ",
        )
        .bind(receiver_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_outgoing_friend_requests(&self, sender_id: &str) -> Result<Vec<FriendRequestRecord>, sqlx::Error> {
        let rows = sqlx::query_as::<_, FriendRequestRecord>(
            r"
            SELECT id, sender_id, receiver_id, message, status, created_ts, updated_ts
            FROM friend_requests
            WHERE sender_id = $1 AND status = 'pending'
            ORDER BY created_ts DESC
            ",
        )
        .bind(sender_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn update_friend_request_status(
        &self,
        sender_id: &str,
        receiver_id: &str,
        status: &str,
    ) -> Result<bool, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let result = sqlx::query(
            r"
            UPDATE friend_requests
            SET status = $3, updated_ts = $4
            WHERE sender_id = $1 AND receiver_id = $2 AND status = 'pending'
            ",
        )
        .bind(sender_id)
        .bind(receiver_id)
        .bind(status)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_friend_request(&self, sender_id: &str, receiver_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r"
            DELETE FROM friend_requests
            WHERE sender_id = $1 AND receiver_id = $2
            ",
        )
        .bind(sender_id)
        .bind(receiver_id)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn has_pending_request(&self, sender_id: &str, receiver_id: &str) -> Result<bool, sqlx::Error> {
        let row = sqlx::query(
            r"
            SELECT 1 FROM friend_requests
            WHERE sender_id = $1 AND receiver_id = $2 AND status = 'pending'
            ",
        )
        .bind(sender_id)
        .bind(receiver_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.is_some())
    }

    pub async fn has_any_pending_request(&self, user_a: &str, user_b: &str) -> Result<bool, sqlx::Error> {
        let row = sqlx::query(
            r"
            SELECT 1 FROM friend_requests
            WHERE ((sender_id = $1 AND receiver_id = $2) OR (sender_id = $2 AND receiver_id = $1))
            AND status = 'pending'
            ",
        )
        .bind(user_a)
        .bind(user_b)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.is_some())
    }

    pub async fn ensure_user_exists(&self, user_id: &str) -> Result<(), sqlx::Error> {
        let existing =
            sqlx::query("SELECT 1 FROM users WHERE user_id = $1").bind(user_id).fetch_optional(&*self.pool).await?;

        if existing.is_none() {
            tracing::warn!("Friend request references non-existent user: {} - refusing to auto-create", user_id);
            return Err(sqlx::Error::RowNotFound);
        }

        Ok(())
    }

    pub async fn create_friend_request_with_user_ensure(
        &self,
        sender_id: &str,
        receiver_id: &str,
        message: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        self.ensure_user_exists(sender_id).await?;
        self.ensure_user_exists(receiver_id).await?;

        self.create_friend_request(sender_id, receiver_id, message).await
    }

    pub async fn get_mutual_friends(&self, user_id: &str, target_user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let user_friends = self.get_user_friend_ids(user_id).await?;
        let target_friends = self.get_user_friend_ids(target_user_id).await?;

        let mutual: Vec<String> = user_friends.into_iter().filter(|f| target_friends.contains(f)).collect();

        Ok(mutual)
    }

    pub async fn get_user_friend_ids(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let room_id = self.get_friend_list_room_id(user_id).await?;

        if let Some(room_id) = room_id {
            let content = self.get_friend_list_content(&room_id).await?;

            if let Some(content) = content {
                if let Some(friends) = content.get("friends").and_then(|f| f.as_array()) {
                    return Ok(friends
                        .iter()
                        .filter_map(|f| f.get("user_id").and_then(|u| u.as_str()).map(|s| s.to_string()))
                        .collect());
                }
            }
        }

        Ok(Vec::new())
    }

    pub async fn get_shared_rooms(&self, user_id: &str, target_user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query(
            r"
            SELECT DISTINCT r1.room_id
            FROM room_memberships r1
            INNER JOIN room_memberships r2 ON r1.room_id = r2.room_id
            WHERE r1.user_id = $1
            AND r2.user_id = $2
            AND r1.membership = 'join'
            AND r2.membership = 'join'
            LIMIT 20
            ",
        )
        .bind(user_id)
        .bind(target_user_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.iter().filter_map(|r| r.try_get("room_id").ok()).collect())
    }

    pub async fn get_friend_suggestions_from_mutual_friends(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            r"
            WITH user_friends AS (
                SELECT DISTINCT jsonb_array_elements(content->'friends')->>'user_id' AS friend_id
                FROM events
                WHERE event_type = 'm.friends.list'
                AND sender = $1
            ),
            friends_of_friends AS (
                SELECT
                    f2.friend_id AS suggested_user,
                    COUNT(DISTINCT f1.friend_id) AS mutual_count
                FROM user_friends f1
                JOIN LATERAL (
                    SELECT DISTINCT jsonb_array_elements(content->'friends')->>'user_id' AS friend_id
                    FROM events
                    WHERE event_type = 'm.friends.list'
                    AND sender = f1.friend_id
                ) f2 ON true
                WHERE f2.friend_id != $1
                AND f2.friend_id NOT IN (SELECT friend_id FROM user_friends)
                GROUP BY f2.friend_id
                ORDER BY mutual_count DESC
                LIMIT $2
            )
            SELECT
                f.suggested_user AS user_id,
                f.mutual_count,
                u.displayname,
                u.avatar_url
            FROM friends_of_friends f
            LEFT JOIN users u ON u.user_id = f.suggested_user
            ",
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "user_id": r.get::<String, _>("user_id"),
                    "displayname": r.get::<Option<String>, _>("displayname"),
                    "avatar_url": r.get::<Option<String>, _>("avatar_url"),
                    "reason": "mutual_friends",
                    "mutual_friends_count": r.get::<i64, _>("mutual_count")
                })
            })
            .collect())
    }

    pub async fn get_friend_suggestions_from_shared_rooms(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            r"
            WITH user_rooms AS (
                SELECT room_id FROM room_memberships
                WHERE user_id = $1 AND membership = 'join'
            ),
            room_users AS (
                SELECT
                    rm.user_id,
                    COUNT(DISTINCT rm.room_id) AS shared_rooms_count
                FROM room_memberships rm
                JOIN user_rooms ur ON rm.room_id = ur.room_id
                WHERE rm.user_id != $1
                AND rm.membership = 'join'
                AND rm.user_id NOT IN (
                    SELECT DISTINCT jsonb_array_elements(content->'friends')->>'user_id'
                    FROM events
                    WHERE event_type = 'm.friends.list'
                    AND sender = $1
                )
                GROUP BY rm.user_id
                ORDER BY shared_rooms_count DESC
                LIMIT $2
            )
            SELECT
                ru.user_id,
                ru.shared_rooms_count,
                u.displayname,
                u.avatar_url
            FROM room_users ru
            LEFT JOIN users u ON u.user_id = ru.user_id
            ",
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "user_id": r.get::<String, _>("user_id"),
                    "displayname": r.get::<Option<String>, _>("displayname"),
                    "avatar_url": r.get::<Option<String>, _>("avatar_url"),
                    "reason": "shared_rooms",
                    "shared_rooms_count": r.get::<i64, _>("shared_rooms_count")
                })
            })
            .collect())
    }
}

#[async_trait]
impl FriendRoomStoreApi for FriendRoomStorage {
    async fn get_friend_list_room_id(&self, user_id: &str) -> Result<Option<String>, sqlx::Error> {
        self.get_friend_list_room_id(user_id).await
    }

    async fn get_friend_list_content(&self, room_id: &str) -> Result<Option<serde_json::Value>, sqlx::Error> {
        self.get_friend_list_content(room_id).await
    }

    async fn find_friend_lists_by_dm_room_id(&self, dm_room_id: &str) -> Result<Vec<FriendDmLink>, sqlx::Error> {
        self.find_friend_lists_by_dm_room_id(dm_room_id).await
    }

    async fn get_effective_direct_links_fallback(
        &self,
        user_id: &str,
    ) -> Result<Vec<DirectRoomFallbackLink>, sqlx::Error> {
        self.get_effective_direct_links_fallback(user_id).await
    }

    async fn get_existing_direct_room_id(&self, user_id: &str, friend_id: &str) -> Result<Option<String>, sqlx::Error> {
        self.get_existing_direct_room_id(user_id, friend_id).await
    }

    async fn get_dm_partner_for_room(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<Option<DmPartnerRecord>, sqlx::Error> {
        self.get_dm_partner_for_room(room_id, user_id).await
    }

    async fn get_friend_requests(
        &self,
        room_id: &str,
        request_type: &str,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        self.get_friend_requests(room_id, request_type).await
    }

    async fn is_friend(&self, room_id: &str, friend_id: &str) -> Result<bool, sqlx::Error> {
        self.is_friend(room_id, friend_id).await
    }

    async fn get_friend_info(&self, room_id: &str, friend_id: &str) -> Result<Option<serde_json::Value>, sqlx::Error> {
        self.get_friend_info(room_id, friend_id).await
    }

    async fn get_friend_groups(&self, room_id: &str) -> Result<Option<serde_json::Value>, sqlx::Error> {
        self.get_friend_groups(room_id).await
    }

    async fn get_friend_groups_for_user(&self, room_id: &str, friend_id: &str) -> Result<Vec<String>, sqlx::Error> {
        self.get_friend_groups_for_user(room_id, friend_id).await
    }

    async fn create_friend_group(&self, room_id: &str, user_id: &str, group_name: &str) -> Result<(), sqlx::Error> {
        self.create_friend_group(room_id, user_id, group_name).await
    }

    async fn delete_friend_group(&self, room_id: &str, user_id: &str, group_name: &str) -> Result<bool, sqlx::Error> {
        self.delete_friend_group(room_id, user_id, group_name).await
    }

    async fn rename_friend_group(
        &self,
        room_id: &str,
        user_id: &str,
        old_name: &str,
        new_name: &str,
    ) -> Result<bool, sqlx::Error> {
        self.rename_friend_group(room_id, user_id, old_name, new_name).await
    }

    async fn add_friend_to_group(
        &self,
        room_id: &str,
        user_id: &str,
        group_name: &str,
        friend_id: &str,
    ) -> Result<bool, sqlx::Error> {
        self.add_friend_to_group(room_id, user_id, group_name, friend_id).await
    }

    async fn remove_friend_from_group(
        &self,
        room_id: &str,
        user_id: &str,
        group_name: &str,
        friend_id: &str,
    ) -> Result<bool, sqlx::Error> {
        self.remove_friend_from_group(room_id, user_id, group_name, friend_id).await
    }

    async fn create_friend_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
        message: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        self.create_friend_request(sender_id, receiver_id, message).await
    }

    async fn get_friend_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
    ) -> Result<Option<FriendRequestRecord>, sqlx::Error> {
        self.get_friend_request(sender_id, receiver_id).await
    }

    async fn get_pending_friend_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
    ) -> Result<Option<FriendRequestRecord>, sqlx::Error> {
        self.get_pending_friend_request(sender_id, receiver_id).await
    }

    async fn get_incoming_friend_requests(&self, receiver_id: &str) -> Result<Vec<FriendRequestRecord>, sqlx::Error> {
        self.get_incoming_friend_requests(receiver_id).await
    }

    async fn get_outgoing_friend_requests(&self, sender_id: &str) -> Result<Vec<FriendRequestRecord>, sqlx::Error> {
        self.get_outgoing_friend_requests(sender_id).await
    }

    async fn update_friend_request_status(
        &self,
        sender_id: &str,
        receiver_id: &str,
        status: &str,
    ) -> Result<bool, sqlx::Error> {
        self.update_friend_request_status(sender_id, receiver_id, status).await
    }

    async fn delete_friend_request(&self, sender_id: &str, receiver_id: &str) -> Result<bool, sqlx::Error> {
        self.delete_friend_request(sender_id, receiver_id).await
    }

    async fn has_pending_request(&self, sender_id: &str, receiver_id: &str) -> Result<bool, sqlx::Error> {
        self.has_pending_request(sender_id, receiver_id).await
    }

    async fn has_any_pending_request(&self, user_a: &str, user_b: &str) -> Result<bool, sqlx::Error> {
        self.has_any_pending_request(user_a, user_b).await
    }

    async fn ensure_user_exists(&self, user_id: &str) -> Result<(), sqlx::Error> {
        self.ensure_user_exists(user_id).await
    }

    async fn create_friend_request_with_user_ensure(
        &self,
        sender_id: &str,
        receiver_id: &str,
        message: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        self.create_friend_request_with_user_ensure(sender_id, receiver_id, message).await
    }

    async fn get_mutual_friends(&self, user_id: &str, target_user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        self.get_mutual_friends(user_id, target_user_id).await
    }

    async fn get_user_friend_ids(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        self.get_user_friend_ids(user_id).await
    }

    async fn get_shared_rooms(&self, user_id: &str, target_user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        self.get_shared_rooms(user_id, target_user_id).await
    }

    async fn get_friend_suggestions_from_mutual_friends(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        self.get_friend_suggestions_from_mutual_friends(user_id, limit).await
    }

    async fn get_friend_suggestions_from_shared_rooms(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        self.get_friend_suggestions_from_shared_rooms(user_id, limit).await
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use serde_json::json;
    use sqlx::postgres::PgPoolOptions;
    use std::env;

    async fn test_pool() -> Arc<Pool<Postgres>> {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    async fn ensure_test_user(pool: &Pool<Postgres>, user_id: &str) {
        let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
        sqlx::query(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, EXTRACT(EPOCH FROM NOW()) * 1000) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(username)
        .execute(pool)
        .await
        .ok();
    }

    async fn ensure_test_room(pool: &Pool<Postgres>, room_id: &str) {
        sqlx::query(
            "INSERT INTO rooms (room_id, room_version, is_public, creator, created_ts) VALUES ($1, '1', false, '@test:localhost', EXTRACT(EPOCH FROM NOW()) * 1000) ON CONFLICT (room_id) DO NOTHING",
        )
        .bind(room_id)
        .execute(pool)
        .await
        .ok();
    }

    async fn insert_event(
        pool: &Pool<Postgres>,
        room_id: &str,
        sender: &str,
        event_type: &str,
        state_key: &str,
        content: &serde_json::Value,
    ) {
        let event_id = format!("${}", uuid::Uuid::new_v4().simple());
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO events (event_id, room_id, sender, event_type, state_key, content, origin_server_ts, depth) VALUES ($1, $2, $3, $4, $5, $6, $7, 1)",
        )
        .bind(&event_id)
        .bind(room_id)
        .bind(sender)
        .bind(event_type)
        .bind(state_key)
        .bind(content)
        .bind(now)
        .execute(pool)
        .await
        .ok();
    }

    async fn cleanup_all(pool: &Pool<Postgres>, suffix: &str) {
        let pattern = format!("%{}%", suffix);
        let _ = sqlx::query("DELETE FROM friend_requests WHERE sender_id LIKE $1 OR receiver_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM friends WHERE user_id LIKE $1 OR friend_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM friend_categories WHERE user_id LIKE $1").bind(&pattern).execute(pool).await;
        let _ = sqlx::query("DELETE FROM events WHERE sender LIKE $1 OR room_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM room_memberships WHERE user_id LIKE $1 OR room_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM room_summaries WHERE room_id LIKE $1").bind(&pattern).execute(pool).await;
        let _ = sqlx::query("DELETE FROM rooms WHERE room_id LIKE $1 OR creator LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE user_id LIKE $1").bind(&pattern).execute(pool).await;
    }

    // ——————————————————————————————————————————————
    // get_friend_list_room_id
    // ——————————————————————————————————————————————

    #[tokio::test]
    async fn test_get_friend_list_room_id() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_all(&pool, &suffix).await;

        let user_id = format!("@fr_test_{suffix}:localhost");
        let room_id = format!("!fr_room_{suffix}:localhost");
        ensure_test_user(&pool, &user_id).await;
        ensure_test_room(&pool, &room_id).await;

        // Insert m.room.create with type=m.friends
        let content = json!({"type": "m.friends", "creator": &user_id});
        insert_event(&pool, &room_id, &user_id, "m.room.create", "", &content).await;

        let storage = FriendRoomStorage::new(pool.clone());

        // Found
        let result = storage.get_friend_list_room_id(&user_id).await.expect("query should succeed");
        assert_eq!(result.as_deref(), Some(room_id.as_str()), "should find the friend list room");

        // Not found: user with no friend-list create event
        let other = format!("@fr_other_{suffix}:localhost");
        ensure_test_user(&pool, &other).await;
        let result = storage.get_friend_list_room_id(&other).await.expect("query should succeed");
        assert!(result.is_none(), "should not find room for user without friend-list");

        cleanup_all(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————————
    // get_friend_list_content
    // ——————————————————————————————————————————————

    #[tokio::test]
    async fn test_get_friend_list_content() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_all(&pool, &suffix).await;

        let user_id = format!("@fr_test_{suffix}:localhost");
        let room_id = format!("!fr_room_{suffix}:localhost");
        ensure_test_room(&pool, &room_id).await;

        // Insert a friend-list event with content
        let friend_content = json!({
            "friends": [
                {"user_id": format!("@friend_{suffix}:localhost"), "display_name": "Friend One"}
            ],
            "version": 1
        });
        insert_event(&pool, &room_id, &user_id, "m.friends.list", "", &friend_content).await;

        let storage = FriendRoomStorage::new(pool.clone());

        // Found with content
        let result = storage.get_friend_list_content(&room_id).await.expect("query should succeed");
        assert!(result.is_some(), "should find friend list content");
        let content = result.unwrap();
        let friends = content.get("friends").and_then(|f| f.as_array());
        assert!(friends.is_some_and(|f| f.len() == 1), "should have one friend");

        // Not found: room with no friend-list event
        let other_room = format!("!fr_other_{suffix}:localhost");
        ensure_test_room(&pool, &other_room).await;
        let result = storage.get_friend_list_content(&other_room).await.expect("query should succeed");
        assert!(result.is_none(), "should return None for room without friend-list");

        cleanup_all(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————————
    // find_friend_lists_by_dm_room_id
    // ——————————————————————————————————————————————

    #[tokio::test]
    async fn test_find_friend_lists_by_dm_room_id() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_all(&pool, &suffix).await;

        let user_id = format!("@fr_test_{suffix}:localhost");
        let friend_room_id = format!("!fr_room_{suffix}:localhost");
        let dm_room_id = format!("!dm_room_{suffix}:localhost");
        ensure_test_user(&pool, &user_id).await;
        ensure_test_room(&pool, &friend_room_id).await;

        let friend_content = json!({
            "friends": [
                {"user_id": format!("@friend_{suffix}:localhost"), "dm_room_id": dm_room_id}
            ]
        });
        insert_event(&pool, &friend_room_id, &user_id, "m.friends.list", "", &friend_content).await;

        let storage = FriendRoomStorage::new(pool.clone());

        // Finds
        let results = storage.find_friend_lists_by_dm_room_id(&dm_room_id).await.expect("query should succeed");
        assert_eq!(results.len(), 1, "should find one friend list");
        assert_eq!(results[0].owner_user_id, user_id);

        // Not found: DM room that is not in any friend list
        let other_dm = format!("!dm_other_{suffix}:localhost");
        let results = storage.find_friend_lists_by_dm_room_id(&other_dm).await.expect("query should succeed");
        assert!(results.is_empty(), "should return empty for unknown DM room");

        cleanup_all(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————————
    // get_effective_direct_links_fallback
    // ——————————————————————————————————————————————

    #[tokio::test]
    async fn test_get_effective_direct_links_fallback() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_all(&pool, &suffix).await;

        let user_a = format!("@fr_a_{suffix}:localhost");
        let user_b = format!("@fr_b_{suffix}:localhost");
        let dm_room = format!("!dm_{suffix}:localhost");
        ensure_test_user(&pool, &user_a).await;
        ensure_test_user(&pool, &user_b).await;
        ensure_test_room(&pool, &dm_room).await;

        let now = chrono::Utc::now().timestamp_millis();
        // Insert room_summaries with is_direct=true
        sqlx::query(
            "INSERT INTO room_summaries (room_id, is_direct, updated_ts, created_ts) VALUES ($1, true, $2, $2) ON CONFLICT (room_id) DO NOTHING",
        )
        .bind(&dm_room)
        .bind(now)
        .execute(&*pool)
        .await
        .ok();

        // Insert room_memberships for both users
        sqlx::query(
            "INSERT INTO room_memberships (room_id, user_id, membership) VALUES ($1, $2, 'join') ON CONFLICT (room_id, user_id) DO NOTHING",
        )
        .bind(&dm_room)
        .bind(&user_a)
        .execute(&*pool)
        .await
        .ok();
        sqlx::query(
            "INSERT INTO room_memberships (room_id, user_id, membership) VALUES ($1, $2, 'join') ON CONFLICT (room_id, user_id) DO NOTHING",
        )
        .bind(&dm_room)
        .bind(&user_b)
        .execute(&*pool)
        .await
        .ok();

        let storage = FriendRoomStorage::new(pool.clone());

        // Returns results
        let results = storage.get_effective_direct_links_fallback(&user_a).await.expect("query should succeed");
        assert_eq!(results.len(), 1, "should find one direct room");
        assert_eq!(results[0].other_user_id, user_b);
        assert_eq!(results[0].room_id, dm_room);

        // Empty: user with no direct rooms
        let user_c = format!("@fr_c_{suffix}:localhost");
        ensure_test_user(&pool, &user_c).await;
        let results = storage.get_effective_direct_links_fallback(&user_c).await.expect("query should succeed");
        assert!(results.is_empty(), "should return empty for user without direct rooms");

        cleanup_all(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————————
    // get_existing_direct_room_id
    // ——————————————————————————————————————————————

    #[tokio::test]
    async fn test_get_existing_direct_room_id() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_all(&pool, &suffix).await;

        let user_a = format!("@fr_a_{suffix}:localhost");
        let user_b = format!("@fr_b_{suffix}:localhost");
        let dm_room = format!("!dm_{suffix}:localhost");
        ensure_test_user(&pool, &user_a).await;
        ensure_test_user(&pool, &user_b).await;
        ensure_test_room(&pool, &dm_room).await;

        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO room_summaries (room_id, is_direct, updated_ts, created_ts) VALUES ($1, true, $2, $2) ON CONFLICT (room_id) DO NOTHING",
        )
        .bind(&dm_room)
        .bind(now)
        .execute(&*pool)
        .await
        .ok();
        sqlx::query(
            "INSERT INTO room_memberships (room_id, user_id, membership) VALUES ($1, $2, 'join') ON CONFLICT (room_id, user_id) DO NOTHING",
        )
        .bind(&dm_room)
        .bind(&user_a)
        .execute(&*pool)
        .await
        .ok();
        sqlx::query(
            "INSERT INTO room_memberships (room_id, user_id, membership) VALUES ($1, $2, 'join') ON CONFLICT (room_id, user_id) DO NOTHING",
        )
        .bind(&dm_room)
        .bind(&user_b)
        .execute(&*pool)
        .await
        .ok();

        let storage = FriendRoomStorage::new(pool.clone());

        // Found
        let result = storage.get_existing_direct_room_id(&user_a, &user_b).await.expect("query should succeed");
        assert_eq!(result.as_deref(), Some(dm_room.as_str()), "should find the DM room");

        // Not found: pair not sharing a DM
        let user_c = format!("@fr_c_{suffix}:localhost");
        ensure_test_user(&pool, &user_c).await;
        let result = storage.get_existing_direct_room_id(&user_a, &user_c).await.expect("query should succeed");
        assert!(result.is_none(), "should not find room for unrelated pair");

        cleanup_all(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————————
    // get_dm_partner_for_room
    // ——————————————————————————————————————————————

    #[tokio::test]
    async fn test_get_dm_partner_for_room() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_all(&pool, &suffix).await;

        let user_a = format!("@fr_a_{suffix}:localhost");
        let user_b = format!("@fr_b_{suffix}:localhost");
        let room_id = format!("!room_{suffix}:localhost");
        ensure_test_user(&pool, &user_a).await;
        ensure_test_user(&pool, &user_b).await;
        ensure_test_room(&pool, &room_id).await;

        sqlx::query(
            "INSERT INTO room_memberships (room_id, user_id, membership, display_name, avatar_url) VALUES ($1, $2, 'join', 'User A', '') ON CONFLICT (room_id, user_id) DO NOTHING",
        )
        .bind(&room_id)
        .bind(&user_a)
        .execute(&*pool)
        .await
        .ok();
        sqlx::query(
            "INSERT INTO room_memberships (room_id, user_id, membership, display_name, avatar_url) VALUES ($1, $2, 'join', 'User B', '') ON CONFLICT (room_id, user_id) DO NOTHING",
        )
        .bind(&room_id)
        .bind(&user_b)
        .execute(&*pool)
        .await
        .ok();

        let storage = FriendRoomStorage::new(pool.clone());

        // Found
        let partner = storage.get_dm_partner_for_room(&room_id, &user_a).await.expect("query should succeed");
        assert!(partner.is_some(), "should find the partner");
        let p = partner.unwrap();
        assert_eq!(p.user_id, user_b, "partner should be user_b");
        assert_eq!(p.display_name, "User B");

        // Not found: room with only one member
        let solo_room = format!("!solo_{suffix}:localhost");
        let solo_user = format!("@fr_solo_{suffix}:localhost");
        ensure_test_user(&pool, &solo_user).await;
        ensure_test_room(&pool, &solo_room).await;
        sqlx::query(
            "INSERT INTO room_memberships (room_id, user_id, membership) VALUES ($1, $2, 'join') ON CONFLICT (room_id, user_id) DO NOTHING",
        )
        .bind(&solo_room)
        .bind(&solo_user)
        .execute(&*pool)
        .await
        .ok();
        let result = storage.get_dm_partner_for_room(&solo_room, &solo_user).await.expect("query should succeed");
        assert!(result.is_none(), "should return None for room with single member");

        cleanup_all(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————————
    // get_friend_requests (from events)
    // ——————————————————————————————————————————————

    #[tokio::test]
    async fn test_get_friend_requests() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_all(&pool, &suffix).await;

        let user_id = format!("@fr_test_{suffix}:localhost");
        let room_id = format!("!fr_room_{suffix}:localhost");
        ensure_test_user(&pool, &user_id).await;
        ensure_test_room(&pool, &room_id).await;

        let requests_content = json!({
            "requests": [
                {"sender_id": format!("@sender_{suffix}:localhost"), "receiver_id": &user_id, "status": "pending"},
                {"sender_id": format!("@sender2_{suffix}:localhost"), "receiver_id": &user_id, "status": "pending"}
            ]
        });
        insert_event(&pool, &room_id, &user_id, "m.friend_requests.incoming", "", &requests_content).await;

        let storage = FriendRoomStorage::new(pool.clone());

        // Returns list
        let incoming = storage.get_friend_requests(&room_id, "incoming").await.expect("query should succeed");
        assert_eq!(incoming.len(), 2, "should have 2 incoming requests");

        // Empty: different request type
        let outgoing = storage.get_friend_requests(&room_id, "outgoing").await.expect("query should succeed");
        assert!(outgoing.is_empty(), "should return empty for outgoing type");

        // Filters: non-existent type
        let nonsense = storage.get_friend_requests(&room_id, "nonsense").await.expect("query should succeed");
        assert!(nonsense.is_empty(), "should return empty for unknown type");

        cleanup_all(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————————
    // is_friend
    // ——————————————————————————————————————————————

    #[tokio::test]
    async fn test_is_friend() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_all(&pool, &suffix).await;

        let user_id = format!("@fr_test_{suffix}:localhost");
        let friend_id = format!("@friend_{suffix}:localhost");
        let room_id = format!("!fr_room_{suffix}:localhost");
        ensure_test_user(&pool, &user_id).await;
        ensure_test_room(&pool, &room_id).await;

        let friend_content = json!({
            "friends": [
                {"user_id": &friend_id, "display_name": "My Friend"}
            ]
        });
        insert_event(&pool, &room_id, &user_id, "m.friends.list", "", &friend_content).await;

        let storage = FriendRoomStorage::new(pool.clone());

        // True: friend is in list
        assert!(storage.is_friend(&room_id, &friend_id).await.expect("query should succeed"), "should be a friend");

        // False: user not in list
        let stranger = format!("@stranger_{suffix}:localhost");
        assert!(!storage.is_friend(&room_id, &stranger).await.expect("query should succeed"), "should not be a friend");

        cleanup_all(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————————
    // get_friend_info
    // ——————————————————————————————————————————————

    #[tokio::test]
    async fn test_get_friend_info() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_all(&pool, &suffix).await;

        let user_id = format!("@fr_test_{suffix}:localhost");
        let friend_id = format!("@friend_{suffix}:localhost");
        let room_id = format!("!fr_room_{suffix}:localhost");
        ensure_test_user(&pool, &user_id).await;
        ensure_test_room(&pool, &room_id).await;

        let friend_content = json!({
            "friends": [
                {"user_id": &friend_id, "display_name": "My Friend", "avatar_url": "mxc://avatar"}
            ]
        });
        insert_event(&pool, &room_id, &user_id, "m.friends.list", "", &friend_content).await;

        let storage = FriendRoomStorage::new(pool.clone());

        // Found
        let info = storage.get_friend_info(&room_id, &friend_id).await.expect("query should succeed");
        assert!(info.is_some(), "should find friend info");
        let info = info.unwrap();
        assert_eq!(info.get("display_name").and_then(|v| v.as_str()), Some("My Friend"));
        assert_eq!(info.get("avatar_url").and_then(|v| v.as_str()), Some("mxc://avatar"));

        // Not found: friend not in list
        let stranger = format!("@stranger_{suffix}:localhost");
        let info = storage.get_friend_info(&room_id, &stranger).await.expect("query should succeed");
        assert!(info.is_none(), "should return None for non-friend");

        cleanup_all(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————————
    // get_friend_groups
    // ——————————————————————————————————————————————

    #[tokio::test]
    async fn test_get_friend_groups() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_all(&pool, &suffix).await;

        let user_id = format!("@fr_test_{suffix}:localhost");
        let room_id = format!("!fr_room_{suffix}:localhost");
        ensure_test_user(&pool, &user_id).await;
        ensure_test_room(&pool, &room_id).await;

        let groups_content = json!({
            "groups": [
                {"name": "close_friends", "members": [], "created_ts": 1000, "updated_ts": 1000}
            ],
            "version": 1,
            "updated_ts": 1000
        });
        insert_event(&pool, &room_id, &user_id, "m.friends.groups", "", &groups_content).await;

        let storage = FriendRoomStorage::new(pool.clone());

        // With groups
        let result = storage.get_friend_groups(&room_id).await.expect("query should succeed");
        assert!(result.is_some(), "should find groups");
        let groups = result.unwrap();
        let arr = groups.get("groups").and_then(|g| g.as_array());
        assert!(arr.is_some_and(|a| a.len() == 1), "should have one group");

        // None: room with no groups event
        let other_room = format!("!fr_other_{suffix}:localhost");
        ensure_test_room(&pool, &other_room).await;
        let result = storage.get_friend_groups(&other_room).await.expect("query should succeed");
        assert!(result.is_none(), "should return None for room without groups");

        cleanup_all(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————————
    // get_friend_groups_for_user
    // ——————————————————————————————————————————————

    #[tokio::test]
    async fn test_get_friend_groups_for_user() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_all(&pool, &suffix).await;

        let user_id = format!("@fr_test_{suffix}:localhost");
        let friend_id = format!("@friend_{suffix}:localhost");
        let room_id = format!("!fr_room_{suffix}:localhost");
        ensure_test_user(&pool, &user_id).await;
        ensure_test_room(&pool, &room_id).await;

        let groups_content = json!({
            "groups": [
                {"name": "close_friends", "members": [&friend_id], "created_ts": 1000, "updated_ts": 1000},
                {"name": "work", "members": [], "created_ts": 1001, "updated_ts": 1001}
            ],
            "version": 1,
            "updated_ts": 1000
        });
        insert_event(&pool, &room_id, &user_id, "m.friends.groups", "", &groups_content).await;

        let storage = FriendRoomStorage::new(pool.clone());

        // Returns groups
        let group_names = storage.get_friend_groups_for_user(&room_id, &friend_id).await.expect("query should succeed");
        assert_eq!(group_names.len(), 1, "should be in one group");
        assert_eq!(group_names[0], "close_friends");

        // Empty: friend not in any group
        let stranger = format!("@stranger_{suffix}:localhost");
        let group_names = storage.get_friend_groups_for_user(&room_id, &stranger).await.expect("query should succeed");
        assert!(group_names.is_empty(), "should return empty for non-member");

        cleanup_all(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————————
    // create_friend_group
    // ——————————————————————————————————————————————

    #[tokio::test]
    async fn test_create_friend_group() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_all(&pool, &suffix).await;

        let user_id = format!("@fr_test_{suffix}:localhost");
        let room_id = format!("!fr_room_{suffix}:localhost");
        ensure_test_user(&pool, &user_id).await;
        ensure_test_room(&pool, &room_id).await;

        let storage = FriendRoomStorage::new(pool.clone());
        let group_name = format!("test_group_{suffix}");

        // Creates
        storage.create_friend_group(&room_id, &user_id, &group_name).await.expect("create should succeed");

        let groups = storage.get_friend_groups(&room_id).await.expect("query should succeed");
        assert!(groups.is_some(), "groups should exist after create");
        let groups = groups.unwrap();
        let arr = groups.get("groups").and_then(|g| g.as_array()).unwrap();
        assert_eq!(arr.len(), 1, "should have one group");
        assert_eq!(arr[0].get("name").and_then(|n| n.as_str()), Some(group_name.as_str()));

        // Duplicate: creating same name again should error
        let err = storage.create_friend_group(&room_id, &user_id, &group_name).await;
        assert!(err.is_err(), "duplicate create should return error");

        cleanup_all(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————————
    // delete_friend_group
    // ——————————————————————————————————————————————

    #[tokio::test]
    async fn test_delete_friend_group() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_all(&pool, &suffix).await;

        let user_id = format!("@fr_test_{suffix}:localhost");
        let room_id = format!("!fr_room_{suffix}:localhost");
        let group_name = format!("temp_group_{suffix}");
        ensure_test_user(&pool, &user_id).await;
        ensure_test_room(&pool, &room_id).await;

        let storage = FriendRoomStorage::new(pool.clone());

        // Create a group first
        storage.create_friend_group(&room_id, &user_id, &group_name).await.expect("create should succeed");

        // Deletes
        let deleted =
            storage.delete_friend_group(&room_id, &user_id, &group_name).await.expect("delete should succeed");
        assert!(deleted, "should return true when group was deleted");

        let groups = storage.get_friend_groups(&room_id).await.expect("query should succeed");
        let groups_val = groups.unwrap_or(json!({"groups": []}));
        let arr = groups_val.get("groups").and_then(|g| g.as_array()).unwrap();
        assert!(arr.is_empty(), "groups should be empty after delete");

        // Idempotent: delete non-existent group returns false
        let deleted =
            storage.delete_friend_group(&room_id, &user_id, &group_name).await.expect("delete should succeed");
        assert!(!deleted, "should return false when group does not exist");

        cleanup_all(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————————
    // rename_friend_group
    // ——————————————————————————————————————————————

    #[tokio::test]
    async fn test_rename_friend_group() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_all(&pool, &suffix).await;

        let user_id = format!("@fr_test_{suffix}:localhost");
        let room_id = format!("!fr_room_{suffix}:localhost");
        let old_name = format!("old_group_{suffix}");
        let new_name = format!("new_group_{suffix}");
        ensure_test_user(&pool, &user_id).await;
        ensure_test_room(&pool, &room_id).await;

        let storage = FriendRoomStorage::new(pool.clone());
        storage.create_friend_group(&room_id, &user_id, &old_name).await.expect("create should succeed");

        // Renames
        let renamed =
            storage.rename_friend_group(&room_id, &user_id, &old_name, &new_name).await.expect("rename should succeed");
        assert!(renamed, "should return true on successful rename");

        let groups = storage.get_friend_groups(&room_id).await.expect("query should succeed");
        let groups = groups.unwrap();
        let arr = groups.get("groups").and_then(|g| g.as_array()).unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0].get("name").and_then(|n| n.as_str()), Some(new_name.as_str()), "group should have new name");

        // Not found: rename non-existent group
        let fake = format!("fake_group_{suffix}");
        let renamed =
            storage.rename_friend_group(&room_id, &user_id, &fake, "irrelevant").await.expect("rename should succeed");
        assert!(!renamed, "should return false when group not found");

        cleanup_all(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————————
    // add_friend_to_group
    // ——————————————————————————————————————————————

    #[tokio::test]
    async fn test_add_friend_to_group() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_all(&pool, &suffix).await;

        let user_id = format!("@fr_test_{suffix}:localhost");
        let friend_id = format!("@friend_{suffix}:localhost");
        let room_id = format!("!fr_room_{suffix}:localhost");
        let group_name = format!("group_{suffix}");
        ensure_test_user(&pool, &user_id).await;
        ensure_test_room(&pool, &room_id).await;

        let storage = FriendRoomStorage::new(pool.clone());
        storage.create_friend_group(&room_id, &user_id, &group_name).await.expect("create should succeed");

        // Add friend
        let added =
            storage.add_friend_to_group(&room_id, &user_id, &group_name, &friend_id).await.expect("add should succeed");
        assert!(added, "should return true when friend was added");

        let group_names = storage.get_friend_groups_for_user(&room_id, &friend_id).await.expect("query should succeed");
        assert!(group_names.contains(&group_name), "friend should be in the group");

        // Add same friend again should return false
        let added_again =
            storage.add_friend_to_group(&room_id, &user_id, &group_name, &friend_id).await.expect("add should succeed");
        assert!(!added_again, "should return false for duplicate addition");

        cleanup_all(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————————
    // remove_friend_from_group
    // ——————————————————————————————————————————————

    #[tokio::test]
    async fn test_remove_friend_from_group() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_all(&pool, &suffix).await;

        let user_id = format!("@fr_test_{suffix}:localhost");
        let friend_id = format!("@friend_{suffix}:localhost");
        let room_id = format!("!fr_room_{suffix}:localhost");
        let group_name = format!("group_{suffix}");
        ensure_test_user(&pool, &user_id).await;
        ensure_test_room(&pool, &room_id).await;

        let storage = FriendRoomStorage::new(pool.clone());
        storage.create_friend_group(&room_id, &user_id, &group_name).await.expect("create should succeed");
        storage.add_friend_to_group(&room_id, &user_id, &group_name, &friend_id).await.expect("add should succeed");

        // Remove friend
        let removed = storage
            .remove_friend_from_group(&room_id, &user_id, &group_name, &friend_id)
            .await
            .expect("remove should succeed");
        assert!(removed, "should return true when friend was removed");

        let group_names = storage.get_friend_groups_for_user(&room_id, &friend_id).await.expect("query should succeed");
        assert!(!group_names.contains(&group_name), "friend should no longer be in the group");

        // Remove again should return false
        let removed_again = storage
            .remove_friend_from_group(&room_id, &user_id, &group_name, &friend_id)
            .await
            .expect("remove should succeed");
        assert!(!removed_again, "should return false when friend already removed");

        cleanup_all(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————————
    // create_friend_request (friend_requests table)
    // ——————————————————————————————————————————————

    #[tokio::test]
    async fn test_create_friend_request() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_all(&pool, &suffix).await;

        let sender = format!("@fr_sender_{suffix}:localhost");
        let receiver = format!("@fr_receiver_{suffix}:localhost");
        ensure_test_user(&pool, &sender).await;
        ensure_test_user(&pool, &receiver).await;

        let storage = FriendRoomStorage::new(pool.clone());

        // Creates
        let id =
            storage.create_friend_request(&sender, &receiver, Some("Hello!")).await.expect("create should succeed");
        assert!(id > 0, "should return a valid ID");

        // Duplicate: upsert updates the status
        let id2 = storage
            .create_friend_request(&sender, &receiver, Some("Updated message"))
            .await
            .expect("upsert should succeed");
        assert_eq!(id2, id, "upsert should return same ID");

        // Verify the record was updated
        let record = storage.get_friend_request(&sender, &receiver).await.expect("query should succeed");
        assert!(record.is_some());
        let record = record.unwrap();
        assert_eq!(record.status, "pending");
        assert_eq!(record.message.as_deref(), Some("Updated message"));

        cleanup_all(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————————
    // get_friend_request
    // ——————————————————————————————————————————————

    #[tokio::test]
    async fn test_get_friend_request() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_all(&pool, &suffix).await;

        let sender = format!("@fr_sender_{suffix}:localhost");
        let receiver = format!("@fr_receiver_{suffix}:localhost");
        ensure_test_user(&pool, &sender).await;
        ensure_test_user(&pool, &receiver).await;

        let storage = FriendRoomStorage::new(pool.clone());
        storage.create_friend_request(&sender, &receiver, Some("Hi")).await.expect("create should succeed");

        // Found
        let record = storage.get_friend_request(&sender, &receiver).await.expect("query should succeed");
        assert!(record.is_some(), "should find the friend request");
        let r = record.unwrap();
        assert_eq!(r.sender_id, sender);
        assert_eq!(r.receiver_id, receiver);
        assert_eq!(r.status, "pending");

        // Not found: non-existent pair
        let other = format!("@fr_other_{suffix}:localhost");
        ensure_test_user(&pool, &other).await;
        let record = storage.get_friend_request(&sender, &other).await.expect("query should succeed");
        assert!(record.is_none(), "should return None for non-existent request");

        cleanup_all(&pool, &suffix).await;
    }

    // ——————————————————————————————————————————————
    // get_pending_friend_request
    // ——————————————————————————————————————————————

    #[tokio::test]
    async fn test_get_pending_friend_request() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_all(&pool, &suffix).await;

        let sender = format!("@fr_sender_{suffix}:localhost");
        let receiver = format!("@fr_receiver_{suffix}:localhost");
        ensure_test_user(&pool, &sender).await;
        ensure_test_user(&pool, &receiver).await;

        let storage = FriendRoomStorage::new(pool.clone());
        storage.create_friend_request(&sender, &receiver, Some("Hi")).await.expect("create should succeed");

        // Found: pending request
        let record = storage.get_pending_friend_request(&sender, &receiver).await.expect("query should succeed");
        assert!(record.is_some(), "should find the pending request");
        assert_eq!(record.unwrap().status, "pending");

        // Not found: non-existent pair
        let stranger = format!("@fr_stranger_{suffix}:localhost");
        ensure_test_user(&pool, &stranger).await;
        let record = storage.get_pending_friend_request(&sender, &stranger).await.expect("query should succeed");
        assert!(record.is_none(), "should return None for pair without pending request");

        // Not found: pair with non-pending status
        storage.update_friend_request_status(&sender, &receiver, "accepted").await.expect("update should succeed");
        let record = storage.get_pending_friend_request(&sender, &receiver).await.expect("query should succeed");
        assert!(record.is_none(), "should not return accepted request as pending");

        cleanup_all(&pool, &suffix).await;
    }
}

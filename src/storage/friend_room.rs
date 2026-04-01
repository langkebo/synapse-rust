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

        self.save_friend_groups(room_id, user_id, &updated_groups)
            .await
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

        self.save_friend_groups(room_id, user_id, &updated_groups)
            .await?;
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

        self.save_friend_groups(room_id, user_id, &updated_groups)
            .await?;
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

        self.save_friend_groups(room_id, user_id, &updated_groups)
            .await?;
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

        self.save_friend_groups(room_id, user_id, &updated_groups)
            .await?;
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
            r#"
            INSERT INTO friend_requests (sender_id, receiver_id, message, status, created_ts)
            VALUES ($1, $2, $3, 'pending', $4)
            ON CONFLICT (sender_id, receiver_id) 
            DO UPDATE SET status = 'pending', updated_ts = $4, message = $3
            RETURNING id
            "#,
        )
        .bind(sender_id)
        .bind(receiver_id)
        .bind(message)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row.get("id"))
    }

    pub async fn get_friend_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
    ) -> Result<Option<FriendRequestRecord>, sqlx::Error> {
        let row = sqlx::query_as::<_, FriendRequestRecord>(
            r#"
            SELECT id, sender_id, receiver_id, message, status, created_ts, updated_ts
            FROM friend_requests
            WHERE sender_id = $1 AND receiver_id = $2
            "#,
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
            r#"
            SELECT id, sender_id, receiver_id, message, status, created_ts, updated_ts
            FROM friend_requests
            WHERE sender_id = $1 AND receiver_id = $2 AND status = 'pending'
            "#,
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
            r#"
            SELECT id, sender_id, receiver_id, message, status, created_ts, updated_ts
            FROM friend_requests
            WHERE receiver_id = $1 AND status = 'pending'
            ORDER BY created_ts DESC
            "#,
        )
        .bind(receiver_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_outgoing_friend_requests(
        &self,
        sender_id: &str,
    ) -> Result<Vec<FriendRequestRecord>, sqlx::Error> {
        let rows = sqlx::query_as::<_, FriendRequestRecord>(
            r#"
            SELECT id, sender_id, receiver_id, message, status, created_ts, updated_ts
            FROM friend_requests
            WHERE sender_id = $1 AND status = 'pending'
            ORDER BY created_ts DESC
            "#,
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
            r#"
            UPDATE friend_requests
            SET status = $3, updated_ts = $4
            WHERE sender_id = $1 AND receiver_id = $2 AND status = 'pending'
            "#,
        )
        .bind(sender_id)
        .bind(receiver_id)
        .bind(status)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_friend_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            DELETE FROM friend_requests
            WHERE sender_id = $1 AND receiver_id = $2
            "#,
        )
        .bind(sender_id)
        .bind(receiver_id)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn has_pending_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT 1 FROM friend_requests
            WHERE sender_id = $1 AND receiver_id = $2 AND status = 'pending'
            "#,
        )
        .bind(sender_id)
        .bind(receiver_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.is_some())
    }

    pub async fn has_any_pending_request(
        &self,
        user_a: &str,
        user_b: &str,
    ) -> Result<bool, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT 1 FROM friend_requests
            WHERE ((sender_id = $1 AND receiver_id = $2) OR (sender_id = $2 AND receiver_id = $1))
            AND status = 'pending'
            "#,
        )
        .bind(user_a)
        .bind(user_b)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.is_some())
    }

    pub async fn ensure_user_exists(&self, user_id: &str) -> Result<(), sqlx::Error> {
        let existing = sqlx::query("SELECT 1 FROM users WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(&*self.pool)
            .await?;

        if existing.is_none() {
            let now = chrono::Utc::now().timestamp_millis();
            let username = user_id
                .strip_prefix('@')
                .and_then(|s| s.split(':').next())
                .unwrap_or("remote_user");

            sqlx::query(
                r#"
                INSERT INTO users (user_id, username, created_ts, generation)
                VALUES ($1, $2, $3, $3)
                ON CONFLICT (user_id) DO NOTHING
                "#,
            )
            .bind(user_id)
            .bind(username)
            .bind(now)
            .execute(&*self.pool)
            .await?;
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

    pub async fn get_mutual_friends(
        &self,
        user_id: &str,
        target_user_id: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        let user_friends = self.get_user_friend_ids(user_id).await?;
        let target_friends = self.get_user_friend_ids(target_user_id).await?;

        let mutual: Vec<String> = user_friends
            .into_iter()
            .filter(|f| target_friends.contains(f))
            .collect();

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

    pub async fn get_shared_rooms(
        &self,
        user_id: &str,
        target_user_id: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT r1.room_id
            FROM room_memberships r1
            INNER JOIN room_memberships r2 ON r1.room_id = r2.room_id
            WHERE r1.user_id = $1 
            AND r2.user_id = $2
            AND r1.membership = 'join'
            AND r2.membership = 'join'
            LIMIT 20
            "#,
        )
        .bind(user_id)
        .bind(target_user_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .iter()
            .filter_map(|r| r.try_get("room_id").ok())
            .collect())
    }

    pub async fn get_friend_suggestions_from_mutual_friends(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
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
            "#,
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
                    "display_name": r.get::<Option<String>, _>("displayname"),
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
            r#"
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
            "#,
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
                    "display_name": r.get::<Option<String>, _>("displayname"),
                    "avatar_url": r.get::<Option<String>, _>("avatar_url"),
                    "reason": "shared_rooms",
                    "shared_rooms_count": r.get::<i64, _>("shared_rooms_count")
                })
            })
            .collect())
    }
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

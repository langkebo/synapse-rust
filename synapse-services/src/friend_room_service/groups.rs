use super::models::*;
use serde_json::json;
use synapse_common::{ApiError, ApiResult};

impl FriendRoomService {
    fn calculate_suggestion_score(suggestion: &serde_json::Value) -> f64 {
        let mut score = 0.0;

        if let Some(mutual_count) = suggestion.get("mutual_friends_count").and_then(|c| c.as_i64()) {
            score += mutual_count as f64 * 2.0;
        }

        if let Some(room_count) = suggestion.get("shared_rooms_count").and_then(|c| c.as_i64()) {
            score += room_count as f64 * 1.0;
        }

        if suggestion.get("displayname").and_then(|d| d.as_str()).is_some() {
            score += 0.5;
        }

        if suggestion.get("avatar_url").and_then(|a| a.as_str()).is_some() {
            score += 0.3;
        }

        score
    }

    /// 更新好友备注
    pub async fn update_friend_note(&self, user_id: &str, friend_id: &str, note: &str) -> ApiResult<()> {
        let friend_room = self.create_friend_list_room(user_id).await?;

        if !self
            .friend_storage
            .is_friend(&friend_room, friend_id)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to check friendship", &e))?
        {
            return Err(ApiError::not_found(format!("Friend {friend_id} not found in list")));
        }

        let mut content = self
            .friend_storage
            .get_friend_list_content(&friend_room)
            .await
            .map_err(|e| ApiError::database_with_log("Database error", &e))?
            .unwrap_or_else(|| json!({ "friends": [] }));

        if let Some(friends) = content.get_mut("friends").and_then(|f| f.as_array_mut()) {
            for friend in friends.iter_mut() {
                if friend.get("user_id").and_then(|u| u.as_str()) == Some(friend_id) {
                    friend["note"] = json!(note);
                    break;
                }
            }
        }

        self.send_state_event(&friend_room, user_id, "m.friends.list", "", content).await?;

        Ok(())
    }

    /// 更新好友状态 (favorite, normal, blocked, hidden)
    pub async fn update_friend_status(&self, user_id: &str, friend_id: &str, status: &str) -> ApiResult<()> {
        let valid_statuses = ["favorite", "normal", "blocked", "hidden"];
        if !valid_statuses.contains(&status) {
            return Err(ApiError::bad_request(format!(
                "Invalid status '{}'. Valid values: {}",
                status,
                valid_statuses.join(", ")
            )));
        }

        let friend_room = self.create_friend_list_room(user_id).await?;

        if !self
            .friend_storage
            .is_friend(&friend_room, friend_id)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to check friendship", &e))?
        {
            return Err(ApiError::not_found(format!("Friend {friend_id} not found in list")));
        }

        let mut content = self
            .friend_storage
            .get_friend_list_content(&friend_room)
            .await
            .map_err(|e| ApiError::database_with_log("Database error", &e))?
            .unwrap_or_else(|| json!({ "friends": [] }));

        if let Some(friends) = content.get_mut("friends").and_then(|f| f.as_array_mut()) {
            for friend in friends.iter_mut() {
                if friend.get("user_id").and_then(|u| u.as_str()) == Some(friend_id) {
                    friend["status"] = json!(status);
                    friend["status_updated_ts"] = json!(chrono::Utc::now().timestamp_millis());
                    break;
                }
            }
        }

        self.send_state_event(&friend_room, user_id, "m.friends.list", "", content).await?;

        Ok(())
    }

    /// 更新好友显示名
    pub async fn update_friend_displayname(&self, user_id: &str, friend_id: &str, displayname: &str) -> ApiResult<()> {
        let friend_room = self.create_friend_list_room(user_id).await?;

        if !self
            .friend_storage
            .is_friend(&friend_room, friend_id)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to check friendship", &e))?
        {
            return Err(ApiError::not_found(format!("Friend {friend_id} not found in list")));
        }

        let mut content = self
            .friend_storage
            .get_friend_list_content(&friend_room)
            .await
            .map_err(|e| ApiError::database_with_log("Database error", &e))?
            .unwrap_or_else(|| json!({ "friends": [] }));

        if let Some(friends) = content.get_mut("friends").and_then(|f| f.as_array_mut()) {
            for friend in friends.iter_mut() {
                if friend.get("user_id").and_then(|u| u.as_str()) == Some(friend_id) {
                    friend["displayname"] = json!(displayname);
                    friend["displayname_updated_ts"] = json!(chrono::Utc::now().timestamp_millis());
                    break;
                }
            }
        }

        self.send_state_event(&friend_room, user_id, "m.friends.list", "", content).await?;

        Ok(())
    }

    /// 获取好友详细信息
    pub async fn get_friend_info(&self, user_id: &str, friend_id: &str) -> ApiResult<Option<serde_json::Value>> {
        let friend_room = self.create_friend_list_room(user_id).await?;
        self.friend_storage
            .get_friend_info(&friend_room, friend_id)
            .await
            .map_err(|e| ApiError::database_with_log("Database error", &e))
    }

    /// 获取好友状态
    pub async fn get_friend_status(&self, user_id: &str, friend_id: &str) -> ApiResult<serde_json::Value> {
        let friend_room = self.create_friend_list_room(user_id).await?;

        let info = self
            .friend_storage
            .get_friend_info(&friend_room, friend_id)
            .await
            .map_err(|e| ApiError::database_with_log("Database error", &e))?;

        if let Some(info) = info {
            Ok(info)
        } else {
            Ok(json!({
                "user_id": friend_id,
                "status": "none",
                "is_friend": false
            }))
        }
    }

    /// 检查好友关系
    pub async fn check_friendship(&self, user_id: &str, target_id: &str) -> ApiResult<bool> {
        let friend_room = self.create_friend_list_room(user_id).await?;
        self.friend_storage
            .is_friend(&friend_room, target_id)
            .await
            .map_err(|e| ApiError::database_with_log("Database error", &e))
    }

    /// 获取好友推荐
    pub async fn get_friend_suggestions(&self, user_id: &str, limit: Option<i64>) -> ApiResult<Vec<serde_json::Value>> {
        let _friend_room = self.create_friend_list_room(user_id).await?;

        // 规范化请求 limit：默认 20（与历史行为一致），上限 100 以防 DoS。
        let effective_limit = limit.unwrap_or(20).clamp(1, 100);
        // Mutual-friend 池预取 `effective_limit`，room-based fallback 用剩余额度补齐。
        let mutual_fetch_limit = effective_limit;

        let mut suggestions: Vec<serde_json::Value> = Vec::new();
        let mut suggested_user_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

        let mutual_suggestions = self
            .friend_storage
            .get_friend_suggestions_from_mutual_friends(user_id, mutual_fetch_limit)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to get mutual friend suggestions", &e))?;

        for suggestion in mutual_suggestions {
            if let Some(uid) = suggestion.get("user_id").and_then(|u| u.as_str()) {
                suggested_user_ids.insert(uid.to_string());
            }
            suggestions.push(suggestion);
        }

        if (suggestions.len() as i64) < effective_limit {
            let remaining = effective_limit - suggestions.len() as i64;
            let room_suggestions = self
                .friend_storage
                .get_friend_suggestions_from_shared_rooms(user_id, remaining)
                .await
                .map_err(|e| ApiError::database_with_log("Failed to get shared room suggestions", &e))?;

            for suggestion in room_suggestions {
                if let Some(uid) = suggestion.get("user_id").and_then(|u| u.as_str()) {
                    if !suggested_user_ids.contains(uid) {
                        suggested_user_ids.insert(uid.to_string());
                        suggestions.push(suggestion);
                    }
                }
            }
        }

        suggestions.sort_by(|a, b| {
            let score_a = Self::calculate_suggestion_score(a);
            let score_b = Self::calculate_suggestion_score(b);
            score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
        });

        suggestions.truncate(effective_limit as usize);

        Ok(suggestions)
    }

    /// 查询任意用户的好友列表 (支持本地和远程)
    pub async fn query_user_friends(&self, requester_id: &str, target_user_id: &str) -> ApiResult<Vec<String>> {
        if requester_id != target_user_id {
            return Err(ApiError::forbidden("You can only query your own friend list".to_string()));
        }

        let parts: Vec<&str> = target_user_id.split(':').collect();
        if parts.len() < 2 {
            return Err(ApiError::bad_request("Invalid user ID format"));
        }
        let domain = parts[1];

        if domain == self.server_name {
            let friends_json = self.get_friends(target_user_id).await?;
            let friends = friends_json
                .iter()
                .filter_map(|f| f.get("user_id").and_then(|u| u.as_str()).map(|s| s.to_string()))
                .collect();
            return Ok(friends);
        }

        self.federation_client.query_remote_friends(domain, target_user_id).await
    }

    /// 创建好友分组
    pub async fn create_friend_group(&self, user_id: &str, name: &str) -> ApiResult<serde_json::Value> {
        let friend_room = self.create_friend_list_room(user_id).await?;
        let group_id = format!("group_{}_{}", chrono::Utc::now().timestamp_millis(), uuid::Uuid::new_v4());

        let group = json!({
            "id": group_id,
            "name": name,
            "members": [],
            "created_at": chrono::Utc::now().timestamp_millis()
        });

        let mut groups = self
            .friend_storage
            .get_friend_groups(&friend_room)
            .await
            .map_err(|e| ApiError::database_with_log("Database error", &e))?
            .unwrap_or_else(|| json!({ "groups": [] }));

        if let Some(groups_array) = groups.get_mut("groups").and_then(|g| g.as_array_mut()) {
            groups_array.push(group.clone());
        } else {
            groups = json!({ "groups": [group.clone()] });
        }

        self.send_state_event(&friend_room, user_id, "m.friends.groups", "", groups).await?;

        Ok(group)
    }

    /// 删除好友分组
    pub async fn delete_friend_group(&self, user_id: &str, group_id: &str) -> ApiResult<()> {
        let friend_room = self.create_friend_list_room(user_id).await?;
        let mut groups = self
            .friend_storage
            .get_friend_groups(&friend_room)
            .await
            .map_err(|e| ApiError::database_with_log("Database error", &e))?
            .unwrap_or_else(|| json!({ "groups": [] }));

        if let Some(groups_array) = groups.get_mut("groups").and_then(|g| g.as_array_mut()) {
            let original_len = groups_array.len();
            groups_array.retain(|g| g.get("id").and_then(|id| id.as_str()) != Some(group_id));

            if groups_array.len() == original_len {
                return Err(ApiError::not_found(format!("Group {group_id} not found")));
            }

            self.send_state_event(&friend_room, user_id, "m.friends.groups", "", groups).await?;
        }

        Ok(())
    }

    /// 重命名好友分组
    pub async fn rename_friend_group(&self, user_id: &str, group_id: &str, new_name: &str) -> ApiResult<()> {
        let friend_room = self.create_friend_list_room(user_id).await?;
        let mut groups = self
            .friend_storage
            .get_friend_groups(&friend_room)
            .await
            .map_err(|e| ApiError::database_with_log("Database error", &e))?
            .unwrap_or_else(|| json!({ "groups": [] }));

        if let Some(groups_array) = groups.get_mut("groups").and_then(|g| g.as_array_mut()) {
            let mut found = false;
            for group in groups_array.iter_mut() {
                if group.get("id").and_then(|id| id.as_str()) == Some(group_id) {
                    group["name"] = json!(new_name);
                    found = true;
                    break;
                }
            }

            if !found {
                return Err(ApiError::not_found(format!("Group {group_id} not found")));
            }

            self.send_state_event(&friend_room, user_id, "m.friends.groups", "", groups).await?;
        }

        Ok(())
    }

    /// 添加好友到分组
    pub async fn add_friend_to_group(&self, user_id: &str, group_id: &str, friend_id: &str) -> ApiResult<()> {
        let friend_room = self.create_friend_list_room(user_id).await?;

        // 检查好友关系
        if !self
            .friend_storage
            .is_friend(&friend_room, friend_id)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to check friendship", &e))?
        {
            return Err(ApiError::not_found(format!("User {friend_id} is not your friend")));
        }

        let mut groups = self
            .friend_storage
            .get_friend_groups(&friend_room)
            .await
            .map_err(|e| ApiError::database_with_log("Database error", &e))?
            .unwrap_or_else(|| json!({ "groups": [] }));

        if let Some(groups_array) = groups.get_mut("groups").and_then(|g| g.as_array_mut()) {
            let mut found = false;
            for group in groups_array.iter_mut() {
                if group.get("id").and_then(|id| id.as_str()) == Some(group_id) {
                    if let Some(members) = group.get_mut("members").and_then(|m| m.as_array_mut()) {
                        if !members.iter().any(|m| m.as_str() == Some(friend_id)) {
                            members.push(json!(friend_id));
                        }
                    } else {
                        group["members"] = json!([friend_id]);
                    }
                    found = true;
                    break;
                }
            }

            if !found {
                return Err(ApiError::not_found(format!("Group {group_id} not found")));
            }

            self.send_state_event(&friend_room, user_id, "m.friends.groups", "", groups).await?;
        }

        Ok(())
    }

    /// 从分组中移除好友
    pub async fn remove_friend_from_group(&self, user_id: &str, group_id: &str, friend_id: &str) -> ApiResult<()> {
        let friend_room = self.create_friend_list_room(user_id).await?;
        let mut groups = self
            .friend_storage
            .get_friend_groups(&friend_room)
            .await
            .map_err(|e| ApiError::database_with_log("Database error", &e))?
            .unwrap_or_else(|| json!({ "groups": [] }));

        if let Some(groups_array) = groups.get_mut("groups").and_then(|g| g.as_array_mut()) {
            let mut found = false;
            for group in groups_array.iter_mut() {
                if group.get("id").and_then(|id| id.as_str()) == Some(group_id) {
                    if let Some(members) = group.get_mut("members").and_then(|m| m.as_array_mut()) {
                        members.retain(|m| m.as_str() != Some(friend_id));
                    }
                    found = true;
                    break;
                }
            }

            if !found {
                return Err(ApiError::not_found(format!("Group {group_id} not found")));
            }

            self.send_state_event(&friend_room, user_id, "m.friends.groups", "", groups).await?;
        }

        Ok(())
    }

    /// 获取所有好友分组
    pub async fn get_friend_groups(&self, user_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        let friend_room = self.create_friend_list_room(user_id).await?;
        let groups = self
            .friend_storage
            .get_friend_groups(&friend_room)
            .await
            .map_err(|e| ApiError::database_with_log("Database error", &e))?;

        if let Some(g) = groups {
            if let Some(groups_array) = g.get("groups").and_then(|g| g.as_array()) {
                return Ok(groups_array.clone());
            }
        }

        Ok(Vec::new())
    }

    /// 获取用户所在的分组
    pub async fn get_groups_for_user(&self, user_id: &str, friend_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        let friend_room = self.create_friend_list_room(user_id).await?;
        let groups = self
            .friend_storage
            .get_friend_groups(&friend_room)
            .await
            .map_err(|e| ApiError::database_with_log("Database error", &e))?;

        if let Some(g) = groups {
            if let Some(groups_array) = g.get("groups").and_then(|g| g.as_array()) {
                return Ok(groups_array
                    .iter()
                    .filter(|group| {
                        group
                            .get("members")
                            .and_then(|m| m.as_array())
                            .is_some_and(|members| members.iter().any(|m| m.as_str() == Some(friend_id)))
                    })
                    .cloned()
                    .collect());
            }
        }

        Ok(Vec::new())
    }

    /// 获取分组中的好友
    pub async fn get_friends_in_group(&self, user_id: &str, group_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        let friend_room = self.create_friend_list_room(user_id).await?;
        let groups = self
            .friend_storage
            .get_friend_groups(&friend_room)
            .await
            .map_err(|e| ApiError::database_with_log("Database error", &e))?;

        if let Some(group) = groups.iter().find(|g| g.get("id").and_then(|id| id.as_str()) == Some(group_id)) {
            if let Some(members) = group.get("members").and_then(|m| m.as_array()) {
                let friends = self.get_friends(user_id).await?;
                return Ok(friends
                    .into_iter()
                    .filter(|f| members.iter().any(|m| m.as_str() == f.get("user_id").and_then(|u| u.as_str())))
                    .collect());
            }
        }

        Ok(Vec::new())
    }
}

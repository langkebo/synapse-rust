use crate::common::{generate_event_id, ApiError, ApiResult};
use crate::federation::friend::FriendFederationClient;
use crate::services::RoomService;
use crate::storage::{CreateEventParams, EventStorage, FriendRoomStorage};
use serde_json::json;
use std::sync::Arc;

pub struct FriendRoomService {
    friend_storage: FriendRoomStorage,
    room_service: Arc<RoomService>,
    event_storage: EventStorage,
    server_name: String,
    federation_client: Arc<FriendFederationClient>,
}

impl FriendRoomService {
    pub fn new(
        friend_storage: FriendRoomStorage,
        room_service: Arc<RoomService>,
        event_storage: EventStorage,
        server_name: String,
    ) -> Self {
        let federation_client = Arc::new(FriendFederationClient::new(server_name.clone()));
        Self {
            friend_storage,
            room_service,
            event_storage,
            server_name,
            federation_client,
        }
    }

    /// 创建或获取好友列表房间
    pub async fn create_friend_list_room(&self, user_id: &str) -> ApiResult<String> {
        if let Ok(Some(room_id)) = self.friend_storage.get_friend_list_room_id(user_id).await {
            return Ok(room_id);
        }

        let config = crate::services::room_service::CreateRoomConfig {
            name: Some("Friends".to_string()),
            visibility: Some("private".to_string()),
            preset: Some("private_chat".to_string()),
            topic: Some("User Friends List".to_string()),
            room_type: Some("m.friends".to_string()),
            ..Default::default()
        };

        let response = self.room_service.create_room(user_id, config).await?;
        let room_id = response
            .get("room_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::internal("Failed to get room_id from create_room response"))?
            .to_string();

        let content = json!({ "friends": [], "version": 1 });
        self.send_state_event(&room_id, user_id, "m.friends.list", "", content)
            .await?;

        Ok(room_id)
    }

    /// 发送好友请求 (创建 pending 状态的请求)
    pub async fn send_friend_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
        message: Option<&str>,
    ) -> ApiResult<i64> {
        if receiver_id == sender_id {
            return Err(ApiError::bad_request(
                "Cannot send friend request to yourself",
            ));
        }

        let sender_friend_room = self.create_friend_list_room(sender_id).await?;
        if self
            .friend_storage
            .is_friend(&sender_friend_room, receiver_id)
            .await
            .map_err(|e| ApiError::database(format!("Failed to check friendship: {}", e)))?
        {
            return Err(ApiError::conflict(format!(
                "User {} is already your friend",
                receiver_id
            )));
        }

        if self
            .friend_storage
            .has_any_pending_request(sender_id, receiver_id)
            .await
            .map_err(|e| ApiError::database(format!("Failed to check pending request: {}", e)))?
        {
            return Err(ApiError::conflict(
                "A pending friend request already exists between you and this user".to_string(),
            ));
        }

        let request_id = self
            .friend_storage
            .create_friend_request(sender_id, receiver_id, message)
            .await
            .map_err(|e| ApiError::database(format!("Failed to create friend request: {}", e)))?;

        if self.is_remote_user(receiver_id) {
            tracing::info!(
                "Sending remote friend request: {} -> {}",
                sender_id,
                receiver_id
            );
            let parts: Vec<&str> = receiver_id.split(':').collect();
            if parts.len() >= 2 {
                let domain = parts[1];
                let invite_content = json!({
                    "requester": sender_id,
                    "target": receiver_id,
                    "message": message,
                    "timestamp": chrono::Utc::now().timestamp_millis(),
                    "msgtype": "m.friend_request"
                });

                if let Err(e) = self
                    .federation_client
                    .send_invite(domain, "unused", &invite_content)
                    .await
                {
                    tracing::warn!("Failed to send federation friend request: {}", e);
                }
            }
        }

        Ok(request_id)
    }

    /// 接受好友请求
    pub async fn accept_friend_request(
        &self,
        user_id: &str,
        requester_id: &str,
    ) -> ApiResult<String> {
        let _pending_request = self
            .friend_storage
            .get_pending_friend_request(requester_id, user_id)
            .await
            .map_err(|e| ApiError::database(format!("Failed to get friend request: {}", e)))?
            .ok_or_else(|| {
                ApiError::not_found(format!("No pending friend request from {}", requester_id))
            })?;

        // 为接受者添加好友关系
        let dm_room_id = self.add_friend_internal(user_id, requester_id).await?;

        // 为请求者添加好友关系（双向好友）
        self.add_friend_internal(requester_id, user_id).await?;

        self.friend_storage
            .update_friend_request_status(requester_id, user_id, "accepted")
            .await
            .map_err(|e| ApiError::database(format!("Failed to update request status: {}", e)))?;

        if self.is_remote_user(requester_id) {
            let parts: Vec<&str> = requester_id.split(':').collect();
            if parts.len() >= 2 {
                let domain = parts[1];
                let accept_content = json!({
                    "requester": requester_id,
                    "accepter": user_id,
                    "timestamp": chrono::Utc::now().timestamp_millis(),
                    "msgtype": "m.friend_request.accepted"
                });

                if let Err(e) = self
                    .federation_client
                    .send_invite(domain, "unused", &accept_content)
                    .await
                {
                    tracing::warn!("Failed to send federation friend accept: {}", e);
                }
            }
        }

        Ok(dm_room_id)
    }

    /// 拒绝好友请求
    pub async fn reject_friend_request(&self, user_id: &str, requester_id: &str) -> ApiResult<()> {
        let updated = self
            .friend_storage
            .update_friend_request_status(requester_id, user_id, "rejected")
            .await
            .map_err(|e| ApiError::database(format!("Failed to reject friend request: {}", e)))?;

        if !updated {
            return Err(ApiError::not_found(format!(
                "No pending friend request from {}",
                requester_id
            )));
        }

        Ok(())
    }

    /// 取消发出的好友请求
    pub async fn cancel_friend_request(&self, user_id: &str, target_id: &str) -> ApiResult<()> {
        let updated = self
            .friend_storage
            .update_friend_request_status(user_id, target_id, "cancelled")
            .await
            .map_err(|e| ApiError::database(format!("Failed to cancel friend request: {}", e)))?;

        if !updated {
            return Err(ApiError::not_found(format!(
                "No pending friend request to {}",
                target_id
            )));
        }

        Ok(())
    }

    /// 获取收到的好友请求列表
    pub async fn get_incoming_requests(&self, user_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        let requests = self
            .friend_storage
            .get_incoming_friend_requests(user_id)
            .await
            .map_err(|e| ApiError::database(format!("Database error: {}", e)))?;

        Ok(requests
            .into_iter()
            .map(|r| {
                json!({
                    "user_id": r.sender_id,
                    "message": r.message,
                    "timestamp": r.created_ts,
                    "status": r.status
                })
            })
            .collect())
    }

    /// 获取发出的好友请求列表
    pub async fn get_outgoing_requests(&self, user_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        let requests = self
            .friend_storage
            .get_outgoing_friend_requests(user_id)
            .await
            .map_err(|e| ApiError::database(format!("Database error: {}", e)))?;

        Ok(requests
            .into_iter()
            .map(|r| {
                json!({
                    "user_id": r.receiver_id,
                    "message": r.message,
                    "timestamp": r.created_ts,
                    "status": r.status
                })
            })
            .collect())
    }

    /// 内部方法：添加好友（不检查请求）
    async fn add_friend_internal(&self, user_id: &str, friend_id: &str) -> ApiResult<String> {
        let user_friend_room = self.create_friend_list_room(user_id).await?;

        if self
            .friend_storage
            .is_friend(&user_friend_room, friend_id)
            .await
            .map_err(|e| ApiError::database(format!("Failed to check friendship: {}", e)))?
        {
            let config = crate::services::room_service::CreateRoomConfig {
                visibility: Some("private".to_string()),
                preset: Some("trusted_private_chat".to_string()),
                invite_list: Some(vec![friend_id.to_string()]),
                is_direct: Some(true),
                ..Default::default()
            };

            let response = self.room_service.create_room(user_id, config).await?;
            return Ok(response
                .get("room_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ApiError::internal("Failed to get room_id for DM"))?
                .to_string());
        }

        let config = crate::services::room_service::CreateRoomConfig {
            visibility: Some("private".to_string()),
            preset: Some("trusted_private_chat".to_string()),
            invite_list: Some(vec![friend_id.to_string()]),
            is_direct: Some(true),
            ..Default::default()
        };

        let response = self.room_service.create_room(user_id, config).await?;
        let dm_room_id = response
            .get("room_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::internal("Failed to get room_id for DM"))?
            .to_string();

        self.update_friend_list(user_id, &user_friend_room, friend_id, "add")
            .await?;

        Ok(dm_room_id)
    }

    /// 添加好友 (直接添加，用于向后兼容)
    pub async fn add_friend(&self, user_id: &str, friend_id: &str) -> ApiResult<String> {
        if friend_id == user_id {
            return Err(ApiError::bad_request("Cannot add yourself as a friend"));
        }

        let user_friend_room = self.create_friend_list_room(user_id).await?;

        if self
            .friend_storage
            .is_friend(&user_friend_room, friend_id)
            .await
            .map_err(|e| ApiError::database(format!("Failed to check friendship: {}", e)))?
        {
            return Err(ApiError::conflict(format!(
                "User {} is already your friend",
                friend_id
            )));
        }

        let config = crate::services::room_service::CreateRoomConfig {
            visibility: Some("private".to_string()),
            preset: Some("trusted_private_chat".to_string()),
            invite_list: Some(vec![friend_id.to_string()]),
            is_direct: Some(true),
            ..Default::default()
        };

        let response = self.room_service.create_room(user_id, config).await?;
        let dm_room_id = response
            .get("room_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::internal("Failed to get room_id for DM"))?
            .to_string();

        self.update_friend_list(user_id, &user_friend_room, friend_id, "add")
            .await?;

        if self.is_remote_user(friend_id) {
            tracing::info!("Adding remote friend: {} -> {}", user_id, friend_id);
            let parts: Vec<&str> = friend_id.split(':').collect();
            if parts.len() < 2 {
                return Err(ApiError::bad_request("Invalid user ID format"));
            }
            let domain = parts[1];

            let invite_content = json!({
                "requester": user_id,
                "target": friend_id,
                "timestamp": chrono::Utc::now().timestamp_millis(),
                "msgtype": "m.friend_request"
            });

            if let Err(e) = self
                .federation_client
                .send_invite(domain, "unused", &invite_content)
                .await
            {
                tracing::warn!("Failed to send federation friend request: {}", e);
            }
        }

        Ok(dm_room_id)
    }

    /// 删除好友
    pub async fn remove_friend(&self, user_id: &str, friend_id: &str) -> ApiResult<()> {
        let friend_room = self.create_friend_list_room(user_id).await?;

        if !self
            .friend_storage
            .is_friend(&friend_room, friend_id)
            .await
            .map_err(|e| ApiError::database(format!("Failed to check friendship: {}", e)))?
        {
            return Err(ApiError::not_found(format!(
                "User {} is not in your friend list",
                friend_id
            )));
        }

        self.update_friend_list(user_id, &friend_room, friend_id, "remove")
            .await?;

        Ok(())
    }

    /// 获取好友列表
    pub async fn get_friends(&self, user_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        let room_id = self.create_friend_list_room(user_id).await?;
        let content = self
            .friend_storage
            .get_friend_list_content(&room_id)
            .await
            .map_err(|e| ApiError::database(format!("Database error: {}", e)))?;

        if let Some(c) = content {
            if let Some(friends) = c.get("friends").and_then(|f| f.as_array()) {
                return Ok(friends.clone());
            }
        }

        Ok(Vec::new())
    }

    /// 更新好友备注
    pub async fn update_friend_note(
        &self,
        user_id: &str,
        friend_id: &str,
        note: &str,
    ) -> ApiResult<()> {
        let friend_room = self.create_friend_list_room(user_id).await?;

        if !self
            .friend_storage
            .is_friend(&friend_room, friend_id)
            .await
            .map_err(|e| ApiError::database(format!("Failed to check friendship: {}", e)))?
        {
            return Err(ApiError::not_found(format!(
                "Friend {} not found in list",
                friend_id
            )));
        }

        let mut content = self
            .friend_storage
            .get_friend_list_content(&friend_room)
            .await
            .map_err(|e| ApiError::database(format!("Database error: {}", e)))?
            .unwrap_or_else(|| json!({ "friends": [] }));

        if let Some(friends) = content.get_mut("friends").and_then(|f| f.as_array_mut()) {
            for friend in friends.iter_mut() {
                if friend.get("user_id").and_then(|u| u.as_str()) == Some(friend_id) {
                    friend["note"] = json!(note);
                    break;
                }
            }
        }

        self.send_state_event(&friend_room, user_id, "m.friends.list", "", content)
            .await?;

        Ok(())
    }

    /// 更新好友状态 (favorite, normal, blocked, hidden)
    pub async fn update_friend_status(
        &self,
        user_id: &str,
        friend_id: &str,
        status: &str,
    ) -> ApiResult<()> {
        let friend_room = self.create_friend_list_room(user_id).await?;

        if !self
            .friend_storage
            .is_friend(&friend_room, friend_id)
            .await
            .map_err(|e| ApiError::database(format!("Failed to check friendship: {}", e)))?
        {
            return Err(ApiError::not_found(format!(
                "Friend {} not found in list",
                friend_id
            )));
        }

        let mut content = self
            .friend_storage
            .get_friend_list_content(&friend_room)
            .await
            .map_err(|e| ApiError::database(format!("Database error: {}", e)))?
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

        self.send_state_event(&friend_room, user_id, "m.friends.list", "", content)
            .await?;

        Ok(())
    }

    /// 获取好友详细信息
    pub async fn get_friend_info(
        &self,
        user_id: &str,
        friend_id: &str,
    ) -> ApiResult<Option<serde_json::Value>> {
        let friend_room = self.create_friend_list_room(user_id).await?;
        self.friend_storage
            .get_friend_info(&friend_room, friend_id)
            .await
            .map_err(|e| ApiError::database(format!("Database error: {}", e)))
    }

    /// 获取好友状态
    pub async fn get_friend_status(
        &self,
        user_id: &str,
        friend_id: &str,
    ) -> ApiResult<serde_json::Value> {
        let friend_room = self.create_friend_list_room(user_id).await?;

        let info = self
            .friend_storage
            .get_friend_info(&friend_room, friend_id)
            .await
            .map_err(|e| ApiError::database(format!("Database error: {}", e)))?;

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
            .map_err(|e| ApiError::database(format!("Database error: {}", e)))
    }

    /// 获取好友推荐
    pub async fn get_friend_suggestions(&self, user_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        let _friend_room = self.create_friend_list_room(user_id).await?;

        Ok(vec![
            json!({
                "user_id": "@suggestion1:example.com",
                "display_name": "Suggested User 1",
                "avatar_url": None::<String>,
                "reason": "mutual_friends",
                "mutual_friends_count": 3
            }),
            json!({
                "user_id": "@suggestion2:example.com",
                "display_name": "Suggested User 2",
                "avatar_url": None::<String>,
                "reason": "shared_rooms",
                "shared_rooms_count": 2
            }),
        ])
    }

    /// 创建好友分组
    pub async fn create_friend_group(
        &self,
        user_id: &str,
        name: &str,
    ) -> ApiResult<serde_json::Value> {
        let friend_room = self.create_friend_list_room(user_id).await?;
        let group_id = format!(
            "group_{}_{}",
            chrono::Utc::now().timestamp_millis(),
            uuid::Uuid::new_v4()
        );

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
            .map_err(|e| ApiError::database(format!("Database error: {}", e)))?
            .unwrap_or_else(|| json!({ "groups": [] }));

        if let Some(groups_array) = groups.get_mut("groups").and_then(|g| g.as_array_mut()) {
            groups_array.push(group.clone());
        } else {
            groups = json!({ "groups": [group.clone()] });
        }

        self.send_state_event(&friend_room, user_id, "m.friends.groups", "", groups)
            .await?;

        Ok(group)
    }

    /// 删除好友分组
    pub async fn delete_friend_group(&self, user_id: &str, group_id: &str) -> ApiResult<()> {
        let friend_room = self.create_friend_list_room(user_id).await?;
        let mut groups = self
            .friend_storage
            .get_friend_groups(&friend_room)
            .await
            .map_err(|e| ApiError::database(format!("Database error: {}", e)))?
            .unwrap_or_else(|| json!({ "groups": [] }));

        if let Some(groups_array) = groups.get_mut("groups").and_then(|g| g.as_array_mut()) {
            let original_len = groups_array.len();
            groups_array.retain(|g| g.get("id").and_then(|id| id.as_str()) != Some(group_id));

            if groups_array.len() == original_len {
                return Err(ApiError::not_found(format!("Group {} not found", group_id)));
            }

            self.send_state_event(&friend_room, user_id, "m.friends.groups", "", groups)
                .await?;
        }

        Ok(())
    }

    /// 重命名好友分组
    pub async fn rename_friend_group(
        &self,
        user_id: &str,
        group_id: &str,
        new_name: &str,
    ) -> ApiResult<()> {
        let friend_room = self.create_friend_list_room(user_id).await?;
        let mut groups = self
            .friend_storage
            .get_friend_groups(&friend_room)
            .await
            .map_err(|e| ApiError::database(format!("Database error: {}", e)))?
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
                return Err(ApiError::not_found(format!("Group {} not found", group_id)));
            }

            self.send_state_event(&friend_room, user_id, "m.friends.groups", "", groups)
                .await?;
        }

        Ok(())
    }

    /// 添加好友到分组
    pub async fn add_friend_to_group(
        &self,
        user_id: &str,
        group_id: &str,
        friend_id: &str,
    ) -> ApiResult<()> {
        let friend_room = self.create_friend_list_room(user_id).await?;

        // 检查好友关系
        if !self
            .friend_storage
            .is_friend(&friend_room, friend_id)
            .await
            .map_err(|e| ApiError::database(format!("Failed to check friendship: {}", e)))?
        {
            return Err(ApiError::not_found(format!(
                "User {} is not your friend",
                friend_id
            )));
        }

        let mut groups = self
            .friend_storage
            .get_friend_groups(&friend_room)
            .await
            .map_err(|e| ApiError::database(format!("Database error: {}", e)))?
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
                return Err(ApiError::not_found(format!("Group {} not found", group_id)));
            }

            self.send_state_event(&friend_room, user_id, "m.friends.groups", "", groups)
                .await?;
        }

        Ok(())
    }

    /// 从分组中移除好友
    pub async fn remove_friend_from_group(
        &self,
        user_id: &str,
        group_id: &str,
        friend_id: &str,
    ) -> ApiResult<()> {
        let friend_room = self.create_friend_list_room(user_id).await?;
        let mut groups = self
            .friend_storage
            .get_friend_groups(&friend_room)
            .await
            .map_err(|e| ApiError::database(format!("Database error: {}", e)))?
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
                return Err(ApiError::not_found(format!("Group {} not found", group_id)));
            }

            self.send_state_event(&friend_room, user_id, "m.friends.groups", "", groups)
                .await?;
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
            .map_err(|e| ApiError::database(format!("Database error: {}", e)))?;

        if let Some(g) = groups {
            if let Some(groups_array) = g.get("groups").and_then(|g| g.as_array()) {
                return Ok(groups_array.clone());
            }
        }

        Ok(Vec::new())
    }

    /// 获取用户所在的分组
    pub async fn get_groups_for_user(
        &self,
        user_id: &str,
        friend_id: &str,
    ) -> ApiResult<Vec<serde_json::Value>> {
        let friend_room = self.create_friend_list_room(user_id).await?;
        let groups = self
            .friend_storage
            .get_friend_groups(&friend_room)
            .await
            .map_err(|e| ApiError::database(format!("Database error: {}", e)))?;

        if let Some(g) = groups {
            if let Some(groups_array) = g.get("groups").and_then(|g| g.as_array()) {
                return Ok(groups_array
                    .iter()
                    .filter(|group| {
                        group
                            .get("members")
                            .and_then(|m| m.as_array())
                            .map(|members| members.iter().any(|m| m.as_str() == Some(friend_id)))
                            .unwrap_or(false)
                    })
                    .cloned()
                    .collect());
            }
        }

        Ok(Vec::new())
    }

    /// 获取分组中的好友
    pub async fn get_friends_in_group(
        &self,
        user_id: &str,
        group_id: &str,
    ) -> ApiResult<Vec<serde_json::Value>> {
        let friend_room = self.create_friend_list_room(user_id).await?;
        let groups = self
            .friend_storage
            .get_friend_groups(&friend_room)
            .await
            .map_err(|e| ApiError::database(format!("Database error: {}", e)))?;

        if let Some(group) = groups
            .iter()
            .find(|g| g.get("id").and_then(|id| id.as_str()) == Some(group_id))
        {
            if let Some(members) = group.get("members").and_then(|m| m.as_array()) {
                let friends = self.get_friends(user_id).await?;
                return Ok(friends
                    .into_iter()
                    .filter(|f| {
                        members
                            .iter()
                            .any(|m| m.as_str() == f.get("user_id").and_then(|u| u.as_str()))
                    })
                    .collect());
            }
        }

        Ok(Vec::new())
    }

    /// 处理收到的好友请求 (Federation)
    pub async fn handle_incoming_friend_request(
        &self,
        user_id: &str,
        requester_id: &str,
        content: serde_json::Value,
    ) -> ApiResult<()> {
        let message = content.get("message").and_then(|m| m.as_str());

        self.friend_storage
            .create_friend_request(requester_id, user_id, message)
            .await
            .map_err(|e| ApiError::database(format!("Failed to create friend request: {}", e)))?;

        Ok(())
    }

    /// 查询任意用户的好友列表 (支持本地和远程)
    pub async fn query_user_friends(&self, user_id: &str) -> ApiResult<Vec<String>> {
        let parts: Vec<&str> = user_id.split(':').collect();
        if parts.len() < 2 {
            return Err(ApiError::bad_request("Invalid user ID format"));
        }
        let domain = parts[1];

        if domain == self.server_name {
            let friends_json = self.get_friends(user_id).await?;
            let friends = friends_json
                .iter()
                .filter_map(|f| {
                    f.get("user_id")
                        .and_then(|u| u.as_str())
                        .map(|s| s.to_string())
                })
                .collect();
            return Ok(friends);
        }

        self.federation_client
            .query_remote_friends(domain, user_id)
            .await
    }

    // --- Helpers ---

    fn is_remote_user(&self, user_id: &str) -> bool {
        !user_id.ends_with(&format!(":{}", self.server_name))
    }

    async fn send_state_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
        state_key: &str,
        content: serde_json::Value,
    ) -> ApiResult<()> {
        let now = chrono::Utc::now().timestamp_millis();
        self.event_storage
            .create_event(
                CreateEventParams {
                    event_id: generate_event_id(&self.server_name),
                    room_id: room_id.to_string(),
                    user_id: user_id.to_string(),
                    event_type: event_type.to_string(),
                    content,
                    state_key: Some(state_key.to_string()),
                    origin_server_ts: now,
                },
                None,
            )
            .await
            .map_err(|e| {
                let error_msg = e.to_string();
                if error_msg.contains("foreign key") {
                    if error_msg.contains("room_id") {
                        ApiError::not_found("Room not found")
                    } else if error_msg.contains("sender") || error_msg.contains("user_id") {
                        ApiError::not_found("User not found")
                    } else {
                        ApiError::database(error_msg)
                    }
                } else {
                    ApiError::database(error_msg)
                }
            })?;
        Ok(())
    }

    async fn update_friend_list(
        &self,
        user_id: &str,
        room_id: &str,
        friend_id: &str,
        action: &str,
    ) -> ApiResult<()> {
        let mut content = self
            .friend_storage
            .get_friend_list_content(room_id)
            .await
            .map_err(|e| ApiError::database(format!("Database error: {}", e)))?
            .unwrap_or_else(|| json!({ "friends": [], "version": 1 }));

        let friends_array = content
            .get_mut("friends")
            .and_then(|f| f.as_array_mut())
            .ok_or_else(|| ApiError::internal("Invalid friend list format"))?;

        if action == "add" {
            let exists = friends_array.iter().any(|f| f["user_id"] == friend_id);
            if !exists {
                friends_array.push(json!({
                    "user_id": friend_id,
                    "since": chrono::Utc::now().timestamp(),
                    "status": "normal",
                    "added_at": chrono::Utc::now().timestamp_millis()
                }));
            }
        } else if action == "remove" {
            friends_array.retain(|f| f["user_id"] != friend_id);
        }

        if let Some(version) = content.get("version").and_then(|v| v.as_i64()) {
            content["version"] = json!(version + 1);
        }

        self.send_state_event(room_id, user_id, "m.friends.list", "", content)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_is_remote_user() {}
}

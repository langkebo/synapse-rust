use crate::common::{generate_event_id, ApiError, ApiResult};
use crate::services::RoomService;
use crate::storage::{CreateEventParams, EventStorage, FriendRoomStorage};
use crate::federation::friend::FriendFederationClient;
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

    /// 添加好友
    pub async fn add_friend(&self, user_id: &str, friend_id: &str) -> ApiResult<String> {
        if friend_id == user_id {
            return Err(ApiError::bad_request("Cannot add yourself as a friend"));
        }

        let user_friend_room = self.create_friend_list_room(user_id).await?;
        
        if self.friend_storage.is_friend(&user_friend_room, friend_id).await
            .map_err(|e| ApiError::database(format!("Failed to check friendship: {}", e)))?
        {
            return Err(ApiError::conflict(format!("User {} is already your friend", friend_id)));
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

        self.update_friend_list(user_id, &user_friend_room, friend_id, "add").await?;

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
            
            if let Err(e) = self.federation_client.send_invite(domain, "unused", &invite_content).await {
                tracing::warn!("Failed to send federation friend request: {}", e);
            }
        }

        Ok(dm_room_id)
    }

    /// 删除好友
    pub async fn remove_friend(&self, user_id: &str, friend_id: &str) -> ApiResult<()> {
        let friend_room = self.create_friend_list_room(user_id).await?;
        
        if !self.friend_storage.is_friend(&friend_room, friend_id).await
            .map_err(|e| ApiError::database(format!("Failed to check friendship: {}", e)))?
        {
            return Err(ApiError::not_found(format!("User {} is not in your friend list", friend_id)));
        }

        self.update_friend_list(user_id, &friend_room, friend_id, "remove").await?;

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
    pub async fn update_friend_note(&self, user_id: &str, friend_id: &str, note: &str) -> ApiResult<()> {
        let friend_room = self.create_friend_list_room(user_id).await?;
        
        if !self.friend_storage.is_friend(&friend_room, friend_id).await
            .map_err(|e| ApiError::database(format!("Failed to check friendship: {}", e)))?
        {
            return Err(ApiError::not_found(format!("Friend {} not found in list", friend_id)));
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

        self.send_state_event(&friend_room, user_id, "m.friends.list", "", content).await?;

        Ok(())
    }

    /// 更新好友状态 (favorite, normal, blocked, hidden)
    pub async fn update_friend_status(&self, user_id: &str, friend_id: &str, status: &str) -> ApiResult<()> {
        let friend_room = self.create_friend_list_room(user_id).await?;
        
        if !self.friend_storage.is_friend(&friend_room, friend_id).await
            .map_err(|e| ApiError::database(format!("Failed to check friendship: {}", e)))?
        {
            return Err(ApiError::not_found(format!("Friend {} not found in list", friend_id)));
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
                    friend["status_updated_at"] = json!(chrono::Utc::now().timestamp_millis());
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
        self.friend_storage.get_friend_info(&friend_room, friend_id).await
            .map_err(|e| ApiError::database(format!("Database error: {}", e)))
    }

    /// 获取收到的好友请求列表
    pub async fn get_incoming_requests(&self, user_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        let friend_room = self.create_friend_list_room(user_id).await?;
        self.friend_storage.get_friend_requests(&friend_room, "incoming").await
            .map_err(|e| ApiError::database(format!("Database error: {}", e)))
    }

    /// 获取发出的好友请求列表
    pub async fn get_outgoing_requests(&self, user_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        let friend_room = self.create_friend_list_room(user_id).await?;
        self.friend_storage.get_friend_requests(&friend_room, "outgoing").await
            .map_err(|e| ApiError::database(format!("Database error: {}", e)))
    }

    /// 拒绝好友请求
    pub async fn reject_friend_request(&self, user_id: &str, requester_id: &str) -> ApiResult<()> {
        let friend_room = self.create_friend_list_room(user_id).await?;
        
        let mut requests = self.friend_storage.get_friend_requests(&friend_room, "incoming").await
            .map_err(|e| ApiError::database(format!("Database error: {}", e)))?;
        
        let original_len = requests.len();
        requests.retain(|r| r.get("user_id").and_then(|u| u.as_str()) != Some(requester_id));
        
        if requests.len() == original_len {
            return Err(ApiError::not_found(format!("No pending request from {}", requester_id)));
        }

        let content = json!({ "requests": requests });
        self.send_state_event(&friend_room, user_id, "m.friend_requests.incoming", "", content).await?;

        Ok(())
    }

    /// 取消发出的好友请求
    pub async fn cancel_friend_request(&self, user_id: &str, target_id: &str) -> ApiResult<()> {
        let friend_room = self.create_friend_list_room(user_id).await?;
        
        let mut requests = self.friend_storage.get_friend_requests(&friend_room, "outgoing").await
            .map_err(|e| ApiError::database(format!("Database error: {}", e)))?;
        
        let original_len = requests.len();
        requests.retain(|r| r.get("user_id").and_then(|u| u.as_str()) != Some(target_id));
        
        if requests.len() == original_len {
            return Err(ApiError::not_found(format!("No pending request to {}", target_id)));
        }

        let content = json!({ "requests": requests });
        self.send_state_event(&friend_room, user_id, "m.friend_requests.outgoing", "", content).await?;

        Ok(())
    }

    /// 处理收到的好友请求 (Federation)
    pub async fn handle_incoming_friend_request(
        &self,
        user_id: &str,
        requester_id: &str,
        content: serde_json::Value,
    ) -> ApiResult<()> {
        let friend_room_id = self.create_friend_list_room(user_id).await?;

        let request_content = json!({
            "user_id": requester_id,
            "content": content,
            "timestamp": chrono::Utc::now().timestamp_millis(),
            "status": "pending"
        });

        let mut current_requests = self.get_state_content(&friend_room_id, "m.friend_requests.incoming", "")
            .await
            .unwrap_or(json!({ "requests": [] }));
        
        let requests_array = current_requests
            .get_mut("requests")
            .and_then(|r| r.as_array_mut());

        if let Some(arr) = requests_array {
            arr.push(request_content);
        } else {
            current_requests = json!({ "requests": [request_content] });
        }

        self.send_state_event(&friend_room_id, user_id, "m.friend_requests.incoming", "", current_requests).await?;

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
            let friends = friends_json.iter()
                .filter_map(|f| f.get("user_id").and_then(|u| u.as_str()).map(|s| s.to_string()))
                .collect();
            return Ok(friends);
        }

        self.federation_client.query_remote_friends(domain, user_id).await
    }

    // --- Helpers ---

    fn is_remote_user(&self, user_id: &str) -> bool {
        !user_id.ends_with(&format!(":{}", self.server_name))
    }

    async fn get_state_content(&self, room_id: &str, event_type: &str, _state_key: &str) -> Option<serde_json::Value> {
        match event_type {
            "m.friends.list" => self.friend_storage.get_friend_list_content(room_id).await.ok().flatten(),
            _ => None,
        }
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

        self.send_state_event(room_id, user_id, "m.friends.list", "", content).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_is_remote_user() {
    }
}

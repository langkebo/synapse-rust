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
        // 1. 检查是否已存在
        if let Ok(Some(room_id)) = self.friend_storage.get_friend_list_room_id(user_id).await {
            return Ok(room_id);
        }

        // 2. 创建新房间
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

        // 3. 初始化空的好友列表事件
        let content = json!({ "friends": [] });
        self.send_state_event(&room_id, user_id, "m.friends.list", "", content)
            .await?;

        Ok(room_id)
    }

    /// 添加好友
    pub async fn add_friend(&self, user_id: &str, friend_id: &str) -> ApiResult<String> {
        // 0. 检查 friend_id 是否为远程用户
        if self.is_remote_user(friend_id) {
             // 远程用户逻辑：发送联邦请求
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
             
             // 发送联邦请求
             self.federation_client.send_invite(domain, "unused", &invite_content).await?;
        }

        // 1. 确保当前用户有好友列表房间
        let user_friend_room = self.create_friend_list_room(user_id).await?;
        
        // 2. 创建私聊房间 (Direct Chat)
        // 如果是远程用户，create_room 会尝试邀请（需要 RoomService 支持联邦邀请）
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

        // 3. 更新好友列表状态事件
        self.update_friend_list(user_id, &user_friend_room, friend_id, "add").await?;

        Ok(dm_room_id)
    }

    /// 处理收到的好友请求 (Federation)
    pub async fn handle_incoming_friend_request(&self, user_id: &str, requester_id: &str, content: serde_json::Value) -> ApiResult<()> {
        // 1. 确保目标用户有好友列表房间
        let friend_room_id = self.create_friend_list_room(user_id).await?;

        // 2. 在好友列表房间中记录请求
        // 使用 m.friend_requests.incoming 事件
        let request_content = json!({
            "requester": requester_id,
            "content": content,
            "timestamp": chrono::Utc::now().timestamp_millis()
        });

        // 获取现有请求
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

    /// 获取好友列表
    pub async fn get_friends(&self, user_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        let room_id = self.create_friend_list_room(user_id).await?;
        let content = self
            .friend_storage
            .get_friend_list_content(&room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        if let Some(c) = content {
            if let Some(friends) = c.get("friends").and_then(|f| f.as_array()) {
                return Ok(friends.clone());
            }
        }

        Ok(Vec::new())
    }

    /// 查询任意用户的好友列表 (支持本地和远程)
    pub async fn query_user_friends(&self, user_id: &str) -> ApiResult<Vec<String>> {
        let parts: Vec<&str> = user_id.split(':').collect();
        if parts.len() < 2 {
            return Err(ApiError::bad_request("Invalid user ID format"));
        }
        let domain = parts[1];
        
        if domain == self.server_name {
             // 本地查询
             let friends_json = self.get_friends(user_id).await?;
             let friends = friends_json.iter()
                .filter_map(|f| f.get("user_id").and_then(|u| u.as_str()).map(|s| s.to_string()))
                .collect();
             return Ok(friends);
        }

        // 远程查询
        self.federation_client.query_remote_friends(domain, user_id).await
    }

    // --- Helpers ---

    fn is_remote_user(&self, user_id: &str) -> bool {
        !user_id.ends_with(&format!(":{}", self.server_name))
    }

    async fn get_state_content(&self, _room_id: &str, _event_type: &str, _state_key: &str) -> Option<serde_json::Value> {
         // 复用 event_storage 的查询能力
         // 这里简单实现，实际应该调用 event_storage.get_state_event
         // 由于 event_storage 没有直接暴露 helper，我们假设可以直接查询
         // 为简化，这里暂时返回 None (假设第一次创建)
         // 实际生产代码需要实现 get_state_event logic
         None 
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
            .map_err(|e| ApiError::internal(format!("Failed to create event: {}", e)))?;
        Ok(())
    }

    async fn update_friend_list(
        &self,
        user_id: &str,
        room_id: &str,
        friend_id: &str,
        action: &str,
    ) -> ApiResult<()> {
        // 获取当前列表
        let mut content = self
            .friend_storage
            .get_friend_list_content(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
            .unwrap_or_else(|| json!({ "friends": [] }));

        let friends_array = content
            .get_mut("friends")
            .and_then(|f| f.as_array_mut())
            .ok_or_else(|| ApiError::internal("Invalid friend list format"))?;

        if action == "add" {
            // 检查是否已存在
            let exists = friends_array.iter().any(|f| f["user_id"] == friend_id);
            if !exists {
                friends_array.push(json!({
                    "user_id": friend_id,
                    "since": chrono::Utc::now().timestamp(),
                    "status": "offline" // 初始状态
                }));
            }
        } else if action == "remove" {
            friends_array.retain(|f| f["user_id"] != friend_id);
        }

        // 发送更新事件
        self.send_state_event(room_id, user_id, "m.friends.list", "", content).await?;
        Ok(())
    }
}

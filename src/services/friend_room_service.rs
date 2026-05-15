use crate::cache::CacheManager;
use crate::common::{generate_event_id, ApiError, ApiResult};
use crate::federation::friend::FriendFederationClient;
use crate::federation::KeyRotationManager;
use crate::services::RoomService;
use crate::storage::{
    CreateEventParams, EventStorage, FriendRoomStorage, PresenceStorage, UserStorage,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

const FRIEND_LIST_CACHE_TTL_SECS: u64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendListRequest {
    pub limit: usize,
    pub offset: usize,
    pub sort_by: String,
}

impl Default for FriendListRequest {
    fn default() -> Self {
        Self {
            limit: 50,
            offset: 0,
            sort_by: "alphabet".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendListEntry {
    pub user_id: String,
    pub username: Option<String>,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub note: Option<String>,
    pub status: String,
    pub online: bool,
    pub presence: String,
    pub last_active_ts: Option<i64>,
    pub last_seen_ts: Option<i64>,
    pub added_ts: Option<i64>,
    pub sort_letter: String,
    pub dm_room_id: Option<String>,
    pub dm_room_active: bool,
    pub dm_room_state: Option<String>,
    pub dm_room_updated_ts: Option<i64>,
    pub dm_room_affected_user_id: Option<String>,
    pub dm_room_changed_by: Option<String>,
    pub dm_room_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendListPage {
    pub room_id: String,
    pub items: Vec<FriendListEntry>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    pub next_offset: Option<usize>,
    pub version: i64,
    pub cached: bool,
    pub generated_ts: i64,
}

pub struct FriendRoomService {
    friend_storage: FriendRoomStorage,
    room_service: Arc<RoomService>,
    event_storage: EventStorage,
    user_storage: UserStorage,
    presence_storage: PresenceStorage,
    cache: Arc<CacheManager>,
    server_name: String,
    federation_client: Arc<FriendFederationClient>,
}

impl FriendRoomService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        friend_storage: FriendRoomStorage,
        room_service: Arc<RoomService>,
        event_storage: EventStorage,
        user_storage: UserStorage,
        presence_storage: PresenceStorage,
        cache: Arc<CacheManager>,
        server_name: String,
        key_rotation_manager: Arc<KeyRotationManager>,
    ) -> Self {
        let federation_client = Arc::new(FriendFederationClient::new(
            server_name.clone(),
            Some(key_rotation_manager),
        ));
        Self {
            friend_storage,
            room_service,
            event_storage,
            user_storage,
            presence_storage,
            cache,
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

        if let Some(msg) = message {
            if msg.len() > 500 {
                return Err(ApiError::bad_request(
                    "Friend request message exceeds maximum length of 500 characters",
                ));
            }
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
            .create_friend_request_with_user_ensure(sender_id, receiver_id, message)
            .await
            .map_err(|e| {
                let error_msg = e.to_string();
                if error_msg.contains("foreign key") || error_msg.contains("no rows returned") {
                    ApiError::not_found(format!(
                        "Cannot send friend request: user not found - {}",
                        receiver_id
                    ))
                } else {
                    ApiError::database(format!("Failed to create friend request: {}", error_msg))
                }
            })?;

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

        let dm_room_id = self.create_friend_dm_room(user_id, requester_id).await?;
        let user_friend_room = self.create_friend_list_room(user_id).await?;
        let requester_friend_room = self.create_friend_list_room(requester_id).await?;

        self.update_friend_list(
            user_id,
            &user_friend_room,
            requester_id,
            "add",
            Some(&dm_room_id),
        )
        .await?;
        self.update_friend_list(
            requester_id,
            &requester_friend_room,
            user_id,
            "add",
            Some(&dm_room_id),
        )
        .await?;

        self.friend_storage
            .update_friend_request_status(requester_id, user_id, "accepted")
            .await
            .map_err(|e| ApiError::database(format!("Failed to update request status: {}", e)))?;

        self.presence_storage
            .add_subscription(user_id, requester_id)
            .await
            .map_err(|e| ApiError::database(format!("Failed to subscribe to presence: {}", e)))?;
        self.presence_storage
            .add_subscription(requester_id, user_id)
            .await
            .map_err(|e| ApiError::database(format!("Failed to subscribe to presence: {}", e)))?;

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

        let dm_room_id = self.create_friend_dm_room(user_id, friend_id).await?;

        self.update_friend_list(
            user_id,
            &user_friend_room,
            friend_id,
            "add",
            Some(&dm_room_id),
        )
        .await?;

        self.presence_storage
            .add_subscription(user_id, friend_id)
            .await
            .map_err(|e| ApiError::database(format!("Failed to subscribe to presence: {}", e)))?;

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

        self.update_friend_list(user_id, &friend_room, friend_id, "remove", None)
            .await?;
        let _ = self
            .presence_storage
            .remove_subscription(user_id, friend_id)
            .await;
        let _ = self
            .presence_storage
            .remove_subscription(friend_id, user_id)
            .await;

        Ok(())
    }

    /// 获取好友列表
    pub async fn get_friends(&self, user_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        let page = self
            .get_friends_page(user_id, FriendListRequest::default())
            .await?;
        Ok(page
            .items
            .into_iter()
            .filter_map(|item| serde_json::to_value(item).ok())
            .collect())
    }

    pub async fn get_friends_page(
        &self,
        user_id: &str,
        request: FriendListRequest,
    ) -> ApiResult<FriendListPage> {
        let room_id = self.create_friend_list_room(user_id).await?;
        let content = self
            .friend_storage
            .get_friend_list_content(&room_id)
            .await
            .map_err(|e| ApiError::database(format!("Database error: {}", e)))?
            .unwrap_or_else(|| json!({ "friends": [], "version": 1 }));

        let version = content.get("version").and_then(|v| v.as_i64()).unwrap_or(1);
        let safe_limit = request.limit.clamp(1, 100);
        let cache_key = format!(
            "friends:list:v2:{}:{}:{}:{}:{}:{}",
            user_id, room_id, version, request.sort_by, request.offset, safe_limit
        );

        if let Ok(Some(mut cached)) = self.cache.get::<FriendListPage>(&cache_key).await {
            cached.cached = true;
            cached.limit = safe_limit;
            return Ok(cached);
        }

        let raw_friends = content
            .get("friends")
            .and_then(|friends| friends.as_array())
            .cloned()
            .unwrap_or_default();
        let friend_ids: Vec<String> = raw_friends
            .iter()
            .filter_map(|friend| {
                friend
                    .get("user_id")
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned)
            })
            .collect();
        let profiles = self
            .user_storage
            .get_user_profiles_map(&friend_ids)
            .await
            .map_err(|e| ApiError::database(format!("Failed to load friend profiles: {}", e)))?;
        let presence_map = self
            .presence_storage
            .get_presence_snapshots(&friend_ids)
            .await
            .map_err(|e| ApiError::database(format!("Failed to load presence snapshots: {}", e)))?;

        let mut items = Self::build_friend_entries(raw_friends, &profiles, &presence_map);
        Self::sort_friend_entries(&mut items, &request.sort_by);

        let total = items.len();
        let offset = request.offset.min(total);
        let paged_items = items
            .into_iter()
            .skip(offset)
            .take(safe_limit)
            .collect::<Vec<_>>();
        let next_offset =
            (offset + paged_items.len() < total).then_some(offset + paged_items.len());

        let page = FriendListPage {
            room_id,
            items: paged_items,
            total,
            limit: safe_limit,
            offset,
            next_offset,
            version,
            cached: false,
            generated_ts: chrono::Utc::now().timestamp_millis(),
        };

        let _ = self
            .cache
            .set(&cache_key, page.clone(), FRIEND_LIST_CACHE_TTL_SECS)
            .await;

        Ok(page)
    }

    pub async fn sync_dm_room_membership_change(
        &self,
        dm_room_id: &str,
        affected_user_id: &str,
        dm_room_state: &str,
        changed_by: Option<&str>,
        reason: Option<&str>,
    ) -> ApiResult<usize> {
        let links = self
            .friend_storage
            .find_friend_lists_by_dm_room_id(dm_room_id)
            .await
            .map_err(|e| ApiError::database(format!("Failed to load friend DM links: {}", e)))?;

        if links.is_empty() {
            return Ok(0);
        }

        let now = chrono::Utc::now().timestamp_millis();
        let mut updated_lists = 0usize;

        for link in links {
            let mut content = link.content;
            let mut touched = false;

            if let Some(friends) = content
                .get_mut("friends")
                .and_then(|value| value.as_array_mut())
            {
                for friend in friends.iter_mut() {
                    if friend.get("dm_room_id").and_then(|value| value.as_str()) != Some(dm_room_id)
                    {
                        continue;
                    }

                    friend["dm_room_state"] = json!(dm_room_state);
                    friend["dm_room_active"] = json!(dm_room_state == "active");
                    friend["dm_room_updated_ts"] = json!(now);
                    friend["dm_room_affected_user_id"] = json!(affected_user_id);

                    if let Some(changed_by) = changed_by {
                        friend["dm_room_changed_by"] = json!(changed_by);
                    }

                    if let Some(reason) = reason {
                        friend["dm_room_reason"] = json!(reason);
                    }

                    touched = true;
                }
            }

            if !touched {
                continue;
            }

            if let Some(version) = content.get("version").and_then(|value| value.as_i64()) {
                content["version"] = json!(version + 1);
            }

            self.send_state_event(
                &link.friend_room_id,
                &link.owner_user_id,
                "m.friends.list",
                "",
                content,
            )
            .await?;
            updated_lists += 1;
        }

        Ok(updated_lists)
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

    /// 更新好友显示名
    pub async fn update_friend_displayname(
        &self,
        user_id: &str,
        friend_id: &str,
        displayname: &str,
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
                    friend["displayname"] = json!(displayname);
                    friend["displayname_updated_ts"] = json!(chrono::Utc::now().timestamp_millis());
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
    pub async fn get_friend_suggestions(
        &self,
        user_id: &str,
        limit: Option<i64>,
    ) -> ApiResult<Vec<serde_json::Value>> {
        let _friend_room = self.create_friend_list_room(user_id).await?;

        // 规范化请求 limit：默认 20（与历史行为一致），上限 100 以防 DoS。
        let effective_limit = limit.unwrap_or(20).clamp(1, 100);
        // Mutual-friend 池预取 `effective_limit`，room-based fallback 用剩余额度补齐。
        let mutual_fetch_limit = effective_limit;

        let mut suggestions: Vec<serde_json::Value> = Vec::new();
        let mut suggested_user_ids: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        let mutual_suggestions = self
            .friend_storage
            .get_friend_suggestions_from_mutual_friends(user_id, mutual_fetch_limit)
            .await
            .map_err(|e| {
                ApiError::database(format!("Failed to get mutual friend suggestions: {}", e))
            })?;

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
                .map_err(|e| {
                    ApiError::database(format!("Failed to get shared room suggestions: {}", e))
                })?;

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
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        suggestions.truncate(effective_limit as usize);

        Ok(suggestions)
    }

    fn calculate_suggestion_score(suggestion: &serde_json::Value) -> f64 {
        let mut score = 0.0;

        if let Some(mutual_count) = suggestion
            .get("mutual_friends_count")
            .and_then(|c| c.as_i64())
        {
            score += mutual_count as f64 * 2.0;
        }

        if let Some(room_count) = suggestion
            .get("shared_rooms_count")
            .and_then(|c| c.as_i64())
        {
            score += room_count as f64 * 1.0;
        }

        if suggestion
            .get("display_name")
            .and_then(|d| d.as_str())
            .is_some()
        {
            score += 0.5;
        }

        if suggestion
            .get("avatar_url")
            .and_then(|a| a.as_str())
            .is_some()
        {
            score += 0.3;
        }

        score
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
            .create_friend_request_with_user_ensure(requester_id, user_id, message)
            .await
            .map_err(|e| {
                let error_msg = e.to_string();
                if error_msg.contains("foreign key") {
                    ApiError::database(format!(
                        "Failed to create friend request: user not found - {}",
                        error_msg
                    ))
                } else {
                    ApiError::database(format!("Failed to create friend request: {}", error_msg))
                }
            })?;

        Ok(())
    }

    /// 查询任意用户的好友列表 (支持本地和远程)
    pub async fn query_user_friends(
        &self,
        requester_id: &str,
        target_user_id: &str,
    ) -> ApiResult<Vec<String>> {
        if requester_id != target_user_id {
            return Err(ApiError::forbidden(
                "You can only query your own friend list".to_string(),
            ));
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
                .filter_map(|f| {
                    f.get("user_id")
                        .and_then(|u| u.as_str())
                        .map(|s| s.to_string())
                })
                .collect();
            return Ok(friends);
        }

        self.federation_client
            .query_remote_friends(domain, target_user_id)
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
        dm_room_id: Option<&str>,
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
                    "added_at": chrono::Utc::now().timestamp_millis(),
                    "dm_room_id": dm_room_id,
                    "dm_room_active": dm_room_id.is_some(),
                    "dm_room_state": if dm_room_id.is_some() { "active" } else { "none" },
                    "dm_room_updated_ts": chrono::Utc::now().timestamp_millis()
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

    async fn create_friend_dm_room(&self, user_id: &str, friend_id: &str) -> ApiResult<String> {
        let config = crate::services::room_service::CreateRoomConfig {
            visibility: Some("private".to_string()),
            preset: Some("trusted_private_chat".to_string()),
            invite_list: Some(vec![friend_id.to_string()]),
            is_direct: Some(true),
            ..Default::default()
        };

        let response = self.room_service.create_room(user_id, config).await?;
        response
            .get("room_id")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned)
            .ok_or_else(|| ApiError::internal("Failed to get room_id for DM"))
    }

    fn build_friend_entries(
        raw_friends: Vec<serde_json::Value>,
        profiles: &HashMap<String, crate::storage::UserProfile>,
        presence_map: &HashMap<String, crate::storage::presence::PresenceSnapshot>,
    ) -> Vec<FriendListEntry> {
        raw_friends
            .into_iter()
            .filter_map(|friend| {
                let user_id = friend.get("user_id")?.as_str()?.to_string();
                let profile = profiles.get(&user_id);
                let displayname = friend
                    .get("displayname")
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned)
                    .or_else(|| profile.and_then(|value| value.displayname.clone()));
                let username = profile.map(|value| value.username.clone());
                let fallback_name = displayname
                    .clone()
                    .or(username.clone())
                    .unwrap_or_else(|| user_id.clone());
                let presence = presence_map
                    .get(&user_id)
                    .map(|snapshot| snapshot.presence.clone())
                    .unwrap_or_else(|| "offline".to_string());
                let last_active_ts = presence_map
                    .get(&user_id)
                    .and_then(|snapshot| snapshot.last_active_ts);

                Some(FriendListEntry {
                    user_id,
                    username,
                    displayname,
                    avatar_url: profile.and_then(|value| value.avatar_url.clone()),
                    note: friend
                        .get("note")
                        .and_then(|value| value.as_str())
                        .map(ToOwned::to_owned),
                    status: friend
                        .get("status")
                        .and_then(|value| value.as_str())
                        .unwrap_or("normal")
                        .to_string(),
                    online: presence == "online",
                    presence,
                    last_active_ts,
                    last_seen_ts: last_active_ts,
                    added_ts: friend.get("added_at").and_then(|value| value.as_i64()),
                    sort_letter: sort_letter_for(&fallback_name),
                    dm_room_id: friend
                        .get("dm_room_id")
                        .and_then(|value| value.as_str())
                        .map(ToOwned::to_owned),
                    dm_room_active: friend
                        .get("dm_room_active")
                        .and_then(|value| value.as_bool())
                        .unwrap_or_else(|| friend.get("dm_room_id").is_some()),
                    dm_room_state: friend
                        .get("dm_room_state")
                        .and_then(|value| value.as_str())
                        .map(ToOwned::to_owned),
                    dm_room_updated_ts: friend
                        .get("dm_room_updated_ts")
                        .and_then(|value| value.as_i64()),
                    dm_room_affected_user_id: friend
                        .get("dm_room_affected_user_id")
                        .and_then(|value| value.as_str())
                        .map(ToOwned::to_owned),
                    dm_room_changed_by: friend
                        .get("dm_room_changed_by")
                        .and_then(|value| value.as_str())
                        .map(ToOwned::to_owned),
                    dm_room_reason: friend
                        .get("dm_room_reason")
                        .and_then(|value| value.as_str())
                        .map(ToOwned::to_owned),
                })
            })
            .collect()
    }

    fn sort_friend_entries(items: &mut [FriendListEntry], sort_by: &str) {
        match sort_by {
            "activity" => items.sort_by(|left, right| {
                right
                    .online
                    .cmp(&left.online)
                    .then_with(|| right.last_active_ts.cmp(&left.last_active_ts))
                    .then_with(|| right.added_ts.cmp(&left.added_ts))
                    .then_with(|| left.user_id.cmp(&right.user_id))
            }),
            "recent" => items.sort_by(|left, right| {
                right
                    .added_ts
                    .cmp(&left.added_ts)
                    .then_with(|| right.last_active_ts.cmp(&left.last_active_ts))
                    .then_with(|| left.user_id.cmp(&right.user_id))
            }),
            _ => items.sort_by(|left, right| {
                left.sort_letter
                    .cmp(&right.sort_letter)
                    .then_with(|| {
                        left.displayname
                            .as_deref()
                            .or(left.username.as_deref())
                            .unwrap_or(left.user_id.as_str())
                            .cmp(
                                right
                                    .displayname
                                    .as_deref()
                                    .or(right.username.as_deref())
                                    .unwrap_or(right.user_id.as_str()),
                            )
                    })
                    .then_with(|| left.user_id.cmp(&right.user_id))
            }),
        }
    }
}

fn sort_letter_for(value: &str) -> String {
    value
        .chars()
        .find(|ch| !ch.is_whitespace())
        .map(|ch| {
            if ch.is_ascii_alphabetic() {
                ch.to_ascii_uppercase().to_string()
            } else {
                "#".to_string()
            }
        })
        .unwrap_or_else(|| "#".to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_is_remote_user() {}

    #[test]
    fn test_sort_letter_for_ascii_name() {
        assert_eq!(super::sort_letter_for("alice"), "A");
    }

    #[test]
    fn test_sort_letter_for_non_ascii_name() {
        assert_eq!(super::sort_letter_for("张三"), "#");
    }
}

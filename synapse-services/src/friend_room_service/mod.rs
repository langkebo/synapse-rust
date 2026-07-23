pub mod groups;
pub mod models;
use self::models::{
    ensure_room_in_direct_map, get_room_direct_users, merge_direct_links, remove_room_from_direct_map, sort_letter_for,
};
pub use models::{
    decode_friend_list_cursor, encode_friend_list_cursor, DirectMapUpdateAction, DirectRoomSnapshot, DmPartnerInfo,
    EnsureDirectRoomResult, FriendListCursor, FriendListEntry, FriendListPage, FriendListRequest,
    FriendRoomCreateRoomConfig, FriendRoomService,
};
use synapse_common::current_timestamp_millis;

use serde_json::{json, Map, Value};

use crate::UserService;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_common::traits::FriendRoomProvider;
use synapse_common::{generate_event_id, ApiError, ApiResult};
use synapse_federation::friend::FriendFederationClient;
use synapse_federation::KeyRotationManager;
use synapse_storage::{CreateEventParams, UserStore};

const FRIEND_LIST_CACHE_TTL_SECS: u64 = 300;
const FRIEND_ROOM_ID_CACHE_TTL_SECS: u64 = 3600;

impl FriendRoomService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        friend_storage: Arc<dyn synapse_storage::friend_room::FriendRoomStoreApi>,
        room_service: Arc<dyn crate::room::RoomServiceApi>,
        user_storage: Arc<dyn UserStore>,
        user_service: Arc<UserService>,
        presence_storage: Arc<dyn synapse_storage::presence::PresenceStoreApi>,
        account_data_storage: Arc<dyn synapse_storage::account_data::AccountDataStoreApi>,
        cache: Arc<CacheManager>,
        server_name: String,
        key_rotation_manager: Arc<KeyRotationManager>,
    ) -> Self {
        let federation_client = Arc::new(FriendFederationClient::new(server_name.clone(), Some(key_rotation_manager)));
        Self::new_with_dependencies(
            friend_storage,
            room_service,
            user_storage,
            user_service,
            presence_storage,
            account_data_storage,
            cache,
            server_name,
            federation_client,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_dependencies(
        friend_storage: Arc<dyn synapse_storage::friend_room::FriendRoomStoreApi>,
        room_service: Arc<dyn crate::room::RoomServiceApi>,
        user_storage: Arc<dyn UserStore>,
        user_service: Arc<UserService>,
        presence_storage: Arc<dyn synapse_storage::presence::PresenceStoreApi>,
        account_data_storage: Arc<dyn synapse_storage::account_data::AccountDataStoreApi>,
        cache: Arc<CacheManager>,
        server_name: String,
        federation_client: Arc<FriendFederationClient>,
    ) -> Self {
        Self {
            friend_storage,
            room_service,
            user_storage,
            user_service,
            presence_storage,
            account_data_storage,
            cache,
            server_name,
            federation_client,
        }
    }

    /// 创建或获取好友列表房间
    pub async fn create_friend_list_room(&self, user_id: &str) -> ApiResult<String> {
        // 先查 Redis 缓存
        let room_cache_key = format!("friends:room_id:{}", user_id);
        if let Ok(Some(room_id)) = self.cache.get::<String>(&room_cache_key).await {
            return Ok(room_id);
        }

        if let Ok(Some(room_id)) = self.friend_storage.get_friend_list_room_id(user_id).await {
            if let Err(e) = self.cache.set(&room_cache_key, room_id.clone(), FRIEND_ROOM_ID_CACHE_TTL_SECS).await {
                ::tracing::warn!(
                    user_id = %user_id,
                    cache_key = %room_cache_key,
                    room_id = %room_id,
                    error = %e,
                    "Failed to cache existing friend list room id"
                );
            }
            return Ok(room_id);
        }

        let config = FriendRoomCreateRoomConfig {
            name: Some("Friends".to_string()),
            visibility: Some("private".to_string()),
            preset: Some("private_chat".to_string()),
            topic: Some("User Friends List".to_string()),
            room_type: Some("m.friends".to_string()),
            ..Default::default()
        };

        let response = self.room_service.lifecycle().create_room(user_id, config.into()).await?;
        let room_id = response
            .get("room_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::internal("Failed to get room_id from create_room response"))?
            .to_string();

        let content = json!({ "friends": [], "version": 1 });
        self.send_state_event(&room_id, user_id, "m.friends.list", "", content).await?;

        // 缓存新创建的 room_id
        let room_cache_key = format!("friends:room_id:{}", user_id);
        if let Err(e) = self.cache.set(&room_cache_key, room_id.clone(), FRIEND_ROOM_ID_CACHE_TTL_SECS).await {
            ::tracing::warn!(
                user_id = %user_id,
                cache_key = %room_cache_key,
                room_id = %room_id,
                error = %e,
                "Failed to cache newly created friend list room id"
            );
        }

        Ok(room_id)
    }

    /// 发送好友请求 (创建 pending 状态的请求)
    #[::tracing::instrument(skip(self, message), fields(request_id = %request_id))]
    pub async fn send_friend_request(
        &self,
        request_id: &str,
        sender_id: &str,
        receiver_id: &str,
        message: Option<&str>,
    ) -> ApiResult<i64> {
        if receiver_id == sender_id {
            return Err(ApiError::bad_request("Cannot send friend request to yourself"));
        }

        if let Some(msg) = message {
            if msg.len() > 500 {
                return Err(ApiError::bad_request("Friend request message exceeds maximum length of 500 characters"));
            }
        }

        let sender_friend_room = self.create_friend_list_room(sender_id).await?;
        if self
            .friend_storage
            .is_friend(&sender_friend_room, receiver_id)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to check friendship", &e))?
        {
            return Err(ApiError::conflict(format!("User {receiver_id} is already your friend")));
        }

        if self
            .friend_storage
            .has_any_pending_request(sender_id, receiver_id)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to check pending request", &e))?
        {
            // Idempotent: return the existing pending request instead of 409
            if let Some(existing) = self
                .friend_storage
                .get_pending_friend_request(sender_id, receiver_id)
                .await
                .map_err(|e| ApiError::database_with_log("Failed to get existing request", &e))?
            {
                tracing::info!(
                    %request_id,
                    sender_id = %sender_id,
                    receiver_id = %receiver_id,
                    friend_request_id = %existing.id,
                    request_direction = %"outgoing",
                    "Returning existing pending friend request"
                );
                return Ok(existing.id);
            }
            // The pending request was sent by the other direction (receiver -> sender)
            if let Some(existing) = self
                .friend_storage
                .get_pending_friend_request(receiver_id, sender_id)
                .await
                .map_err(|e| ApiError::database_with_log("Failed to get existing reverse request", &e))?
            {
                tracing::info!(
                    %request_id,
                    sender_id = %sender_id,
                    receiver_id = %receiver_id,
                    friend_request_id = %existing.id,
                    request_direction = %"incoming",
                    "Returning existing reverse pending friend request"
                );
                return Ok(existing.id);
            }
            // Edge case: pending request disappeared between check and fetch
        }

        let friend_request_id =
            self.friend_storage.create_friend_request_with_user_ensure(sender_id, receiver_id, message).await.map_err(
                |e| {
                    let error_msg = e.to_string();
                    if error_msg.contains("foreign key") || error_msg.contains("no rows returned") {
                        ApiError::not_found(format!("Cannot send friend request: user not found - {receiver_id}"))
                    } else {
                        ApiError::database_with_log("Failed to create friend request", &error_msg)
                    }
                },
            )?;

        if self.is_remote_user(receiver_id) {
            tracing::info!(
                %request_id,
                sender_id = %sender_id,
                receiver_id = %receiver_id,
                remote_delivery = true,
                "Sending remote friend request"
            );
            let parts: Vec<&str> = receiver_id.split(':').collect();
            if parts.len() >= 2 {
                let domain = parts[1];
                let invite_content = json!({
                    "requester": sender_id,
                    "target": receiver_id,
                    "message": message,
                    "timestamp": current_timestamp_millis(),
                    "msgtype": "m.friend_request"
                });

                if let Err(e) = self.federation_client.send_invite(domain, "unused", &invite_content).await {
                    tracing::warn!(
                        %request_id,
                        error = %e,
                        sender_id = %sender_id,
                        receiver_id = %receiver_id,
                        "Failed to send federation friend request"
                    );
                }
            }
        }

        Ok(friend_request_id)
    }

    /// 接受好友请求
    #[::tracing::instrument(skip(self), fields(request_id = %request_id))]
    pub async fn accept_friend_request(
        &self,
        request_id: &str,
        user_id: &str,
        requester_id: &str,
    ) -> ApiResult<String> {
        let _pending_request = self
            .friend_storage
            .get_pending_friend_request(requester_id, user_id)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to get friend request", &e))?
            .ok_or_else(|| ApiError::not_found(format!("No pending friend request from {requester_id}")))?;

        let dm_room_id = self.create_friend_dm_room(user_id, requester_id).await?;
        let user_friend_room = self.create_friend_list_room(user_id).await?;
        let requester_friend_room = self.create_friend_list_room(requester_id).await?;

        self.update_friend_list(user_id, &user_friend_room, requester_id, "add", Some(&dm_room_id)).await?;
        self.update_friend_list(requester_id, &requester_friend_room, user_id, "add", Some(&dm_room_id)).await?;

        self.friend_storage
            .update_friend_request_status(requester_id, user_id, "accepted")
            .await
            .map_err(|e| ApiError::database_with_log("Failed to update request status", &e))?;

        self.presence_storage
            .add_subscription(user_id, requester_id)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to subscribe to presence", &e))?;
        self.presence_storage
            .add_subscription(requester_id, user_id)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to subscribe to presence", &e))?;

        if self.is_remote_user(requester_id) {
            let parts: Vec<&str> = requester_id.split(':').collect();
            if parts.len() >= 2 {
                let domain = parts[1];
                let accept_content = json!({
                    "requester": requester_id,
                    "accepter": user_id,
                    "timestamp": current_timestamp_millis(),
                    "msgtype": "m.friend_request.accepted"
                });

                if let Err(e) = self.federation_client.send_invite(domain, "unused", &accept_content).await {
                    tracing::warn!(
                        %request_id,
                        error = %e,
                        user_id = %user_id,
                        requester_id = %requester_id,
                        "Failed to send federation friend accept"
                    );
                }
            }
        }

        Ok(dm_room_id)
    }

    /// 拒绝好友请求
    #[::tracing::instrument(skip(self), fields(request_id = %request_id))]
    pub async fn reject_friend_request(&self, request_id: &str, user_id: &str, requester_id: &str) -> ApiResult<()> {
        let updated = self
            .friend_storage
            .update_friend_request_status(requester_id, user_id, "rejected")
            .await
            .map_err(|e| ApiError::database_with_log("Failed to reject friend request", &e))?;

        if !updated {
            tracing::warn!(
                %request_id,
                user_id = %user_id,
                requester_id = %requester_id,
                "Reject friend request missed pending row"
            );
            return Err(ApiError::not_found(format!("No pending friend request from {requester_id}")));
        }

        Ok(())
    }

    /// 取消发出的好友请求
    #[::tracing::instrument(skip(self), fields(request_id = %request_id))]
    pub async fn cancel_friend_request(&self, request_id: &str, user_id: &str, target_id: &str) -> ApiResult<()> {
        let updated = self
            .friend_storage
            .update_friend_request_status(user_id, target_id, "cancelled")
            .await
            .map_err(|e| ApiError::database_with_log("Failed to cancel friend request", &e))?;

        if !updated {
            tracing::warn!(
                %request_id,
                user_id = %user_id,
                target_id = %target_id,
                "Cancel friend request missed pending row"
            );
            return Err(ApiError::not_found(format!("No pending friend request to {target_id}")));
        }

        Ok(())
    }

    /// 获取收到的好友请求列表
    pub async fn get_incoming_requests(&self, user_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        let requests = self
            .friend_storage
            .get_incoming_friend_requests(user_id)
            .await
            .map_err(|e| ApiError::database_with_log("Database error", &e))?;

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
            .map_err(|e| ApiError::database_with_log("Database error", &e))?;

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
            .map_err(|e| ApiError::database_with_log("Failed to check friendship", &e))?
        {
            return Err(ApiError::conflict(format!("User {friend_id} is already your friend")));
        }

        let dm_room_id = self.create_friend_dm_room(user_id, friend_id).await?;

        self.update_friend_list(user_id, &user_friend_room, friend_id, "add", Some(&dm_room_id)).await?;

        self.presence_storage
            .add_subscription(user_id, friend_id)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to subscribe to presence", &e))?;

        if self.is_remote_user(friend_id) {
            tracing::info!(user_id = %user_id, friend_id = %friend_id, remote_delivery = true, "Adding remote friend");
            let parts: Vec<&str> = friend_id.split(':').collect();
            if parts.len() < 2 {
                return Err(ApiError::bad_request("Invalid user ID format"));
            }
            let domain = parts[1];

            let invite_content = json!({
                "requester": user_id,
                "target": friend_id,
                "timestamp": current_timestamp_millis(),
                "msgtype": "m.friend_request"
            });

            if let Err(e) = self.federation_client.send_invite(domain, "unused", &invite_content).await {
                tracing::warn!(
                    error = %e,
                    domain = %domain,
                    user_id = %user_id,
                    friend_id = %friend_id,
                    "Failed to send federation friend request"
                );
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
            .map_err(|e| ApiError::database_with_log("Failed to check friendship", &e))?
        {
            return Err(ApiError::not_found(format!("User {friend_id} is not in your friend list")));
        }

        self.update_friend_list(user_id, &friend_room, friend_id, "remove", None).await?;
        let _ = self.presence_storage.remove_subscription(user_id, friend_id).await;
        let _ = self.presence_storage.remove_subscription(friend_id, user_id).await;

        Ok(())
    }

    /// 获取好友列表
    pub async fn get_friends(&self, user_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        let page = self.get_friends_page(user_id, FriendListRequest::default()).await?;
        Ok(page.items.into_iter().filter_map(|item| serde_json::to_value(item).ok()).collect())
    }

    /// 读取用户好友列表里已持久化的 DM 关系。
    ///
    /// 该接口只读取现有好友列表房间，不会像 `create_friend_list_room` 那样
    /// 在只读场景里隐式创建新房间，适合 DM 查询路由的收敛读路径使用。
    pub async fn get_direct_message_links(&self, user_id: &str) -> ApiResult<Vec<(String, String)>> {
        let Some(room_id) = self
            .friend_storage
            .get_friend_list_room_id(user_id)
            .await
            .map_err(|e| ApiError::database_with_log("Database error", &e))?
        else {
            return Ok(Vec::new());
        };

        let content = self
            .friend_storage
            .get_friend_list_content(&room_id)
            .await
            .map_err(|e| ApiError::database_with_log("Database error", &e))?;

        let links = content
            .and_then(|value| value.get("friends").cloned())
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|friend| {
                let friend_id = friend.get("user_id").and_then(|value| value.as_str())?;
                let dm_room_id = friend.get("dm_room_id").and_then(|value| value.as_str())?;
                let is_active = friend.get("dm_room_active").and_then(|value| value.as_bool()).unwrap_or(true);

                is_active.then(|| (friend_id.to_owned(), dm_room_id.to_owned()))
            })
            .collect();

        Ok(links)
    }

    pub async fn load_direct_map(&self, user_id: &str) -> ApiResult<Map<String, Value>> {
        let content = self
            .account_data_storage
            .get_account_data_content(user_id, "m.direct")
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to load m.direct account data", &e))?;

        match content {
            Some(Value::Object(map)) => Ok(map),
            Some(_) => Err(ApiError::internal("Invalid m.direct account data format")),
            None => Ok(Map::new()),
        }
    }

    pub async fn save_direct_map(&self, user_id: &str, direct_map: &Map<String, Value>) -> ApiResult<()> {
        self.account_data_storage
            .upsert_account_data(user_id, "m.direct", Value::Object(direct_map.clone()))
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to save m.direct account data", &e))?;

        // Invalidate the account-data cache for this user so the next /sync
        // will re-read the fresh m.direct data (OPT-015-b, audit 04 §5).
        let _ = self.cache.delete(&format!("account_data:{user_id}")).await;

        Ok(())
    }

    pub async fn get_effective_direct_map(&self, user_id: &str) -> ApiResult<Map<String, Value>> {
        let mut direct_map = self.load_direct_map(user_id).await?;
        merge_direct_links(&mut direct_map, self.get_direct_message_links(user_id).await?);

        if direct_map.is_empty() {
            let rows = self
                .friend_storage
                .get_effective_direct_links_fallback(user_id)
                .await
                .map_err(|e| ApiError::database_with_log("Failed to build effective direct map", &e))?;

            for row in rows {
                ensure_room_in_direct_map(&mut direct_map, &row.other_user_id, &row.room_id);
            }
        }

        Ok(direct_map)
    }

    pub async fn get_direct_room_snapshot(&self, user_id: &str, room_id: &str) -> ApiResult<DirectRoomSnapshot> {
        let direct_map = self.get_effective_direct_map(user_id).await?;
        Ok(Self::build_direct_room_snapshot(direct_map, room_id))
    }

    pub async fn upsert_direct_room_links(
        &self,
        user_id: &str,
        target_user_ids: &[String],
        room_id: &str,
    ) -> ApiResult<Map<String, Value>> {
        let mut direct_map = self.load_direct_map(user_id).await?;
        for target_user_id in target_user_ids {
            ensure_room_in_direct_map(&mut direct_map, target_user_id, room_id);
        }
        self.save_direct_map(user_id, &direct_map).await?;
        Ok(direct_map)
    }

    pub async fn apply_direct_map_update(
        &self,
        user_id: &str,
        action: DirectMapUpdateAction,
    ) -> ApiResult<Map<String, Value>> {
        match action {
            DirectMapUpdateAction::ReplaceRoomTargets { room_id, target_user_ids } => {
                let mut direct_map = self.load_direct_map(user_id).await?;
                remove_room_from_direct_map(&mut direct_map, &room_id);
                for target_user_id in &target_user_ids {
                    ensure_room_in_direct_map(&mut direct_map, target_user_id, &room_id);
                }
                self.save_direct_map(user_id, &direct_map).await?;
                Ok(direct_map)
            }
            DirectMapUpdateAction::OverwriteMap(direct_map) => {
                self.save_direct_map(user_id, &direct_map).await?;
                Ok(direct_map)
            }
        }
    }

    pub async fn update_direct_room_snapshot(
        &self,
        user_id: &str,
        room_id: &str,
        action: DirectMapUpdateAction,
    ) -> ApiResult<DirectRoomSnapshot> {
        let direct_map = self.apply_direct_map_update(user_id, action).await?;
        Ok(Self::build_direct_room_snapshot(direct_map, room_id))
    }

    pub async fn replace_direct_room_targets(
        &self,
        user_id: &str,
        room_id: &str,
        target_user_ids: &[String],
    ) -> ApiResult<Map<String, Value>> {
        self.apply_direct_map_update(
            user_id,
            DirectMapUpdateAction::ReplaceRoomTargets {
                room_id: room_id.to_string(),
                target_user_ids: target_user_ids.to_vec(),
            },
        )
        .await
    }

    pub async fn overwrite_direct_map(
        &self,
        user_id: &str,
        direct_map: Map<String, Value>,
    ) -> ApiResult<Map<String, Value>> {
        self.apply_direct_map_update(user_id, DirectMapUpdateAction::OverwriteMap(direct_map)).await
    }

    /// 当双方已存在好友关系时，将新创建的 DM 房间写回好友列表。
    ///
    /// 这是一个渐进式收敛入口:
    /// - 若不存在好友列表或好友关系，则返回 `0`，不报错
    /// - 若存在单边或双边好友关系，则将对应好友条目的 `dm_room_*` 字段更新为最新值
    pub async fn attach_dm_room_to_existing_friendship(
        &self,
        user_id: &str,
        friend_id: &str,
        dm_room_id: &str,
        changed_by: Option<&str>,
    ) -> ApiResult<usize> {
        let mut updated = 0usize;

        if self.update_existing_friend_dm_link(user_id, friend_id, dm_room_id, "active", changed_by, None).await? {
            updated += 1;
        }

        if self.update_existing_friend_dm_link(friend_id, user_id, dm_room_id, "active", changed_by, None).await? {
            updated += 1;
        }

        Ok(updated)
    }

    /// 查询两名用户之间已存在的 DM 房间。
    ///
    /// 优先读取好友持久化视图中的 `dm_room_id`，若不存在则回退到
    /// `room_memberships + room_summaries` 查询。
    pub async fn get_existing_dm_room_id(&self, user_id: &str, friend_id: &str) -> ApiResult<Option<String>> {
        if let Some(info) = self.get_friend_info(user_id, friend_id).await? {
            let dm_room_id = info.get("dm_room_id").and_then(|value| value.as_str()).map(ToOwned::to_owned);
            let dm_room_active = info.get("dm_room_active").and_then(|value| value.as_bool()).unwrap_or(true);

            if dm_room_active && dm_room_id.is_some() {
                return Ok(dm_room_id);
            }
        }

        self.friend_storage
            .get_existing_direct_room_id(user_id, friend_id)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to query existing DM room", &e))
    }

    pub async fn get_dm_partner_for_room(&self, user_id: &str, room_id: &str) -> ApiResult<Option<DmPartnerInfo>> {
        if let Some((partner_user_id, _)) =
            self.get_direct_message_links(user_id).await?.into_iter().find(|(_, dm_room_id)| dm_room_id == room_id)
        {
            if let Some(profile) = self
                .user_storage
                .get_user_profile(&partner_user_id)
                .await
                .map_err(|e| ApiError::database_with_log("Failed to load DM partner profile", &e))?
            {
                return Ok(Some(DmPartnerInfo {
                    user_id: partner_user_id,
                    display_name: profile.displayname.unwrap_or_default(),
                    avatar_url: profile.avatar_url.unwrap_or_default(),
                }));
            }

            return Ok(Some(DmPartnerInfo {
                user_id: partner_user_id,
                display_name: String::new(),
                avatar_url: String::new(),
            }));
        }

        let partner = self
            .friend_storage
            .get_dm_partner_for_room(room_id, user_id)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to load DM partner from membership", &e))?;

        Ok(partner.map(|row| DmPartnerInfo {
            user_id: row.user_id,
            display_name: row.display_name,
            avatar_url: row.avatar_url,
        }))
    }

    pub async fn ensure_direct_room(
        &self,
        owner_user_id: &str,
        friend_user_id: &str,
        config: FriendRoomCreateRoomConfig,
        actor_user_id: Option<&str>,
    ) -> ApiResult<EnsureDirectRoomResult> {
        if let Some(room_id) = self.get_existing_dm_room_id(owner_user_id, friend_user_id).await? {
            self.attach_dm_room_to_existing_friendship(
                owner_user_id,
                friend_user_id,
                &room_id,
                actor_user_id.or(Some(owner_user_id)),
            )
            .await?;

            return Ok(EnsureDirectRoomResult { room_id, created: false });
        }

        let result = self
            .room_service
            .lifecycle()
            .create_room(owner_user_id, config.into())
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;

        let room_id = result
            .get("room_id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ApiError::internal("Failed to get room_id from create_room response"))?
            .to_string();

        self.attach_dm_room_to_existing_friendship(
            owner_user_id,
            friend_user_id,
            &room_id,
            actor_user_id.or(Some(owner_user_id)),
        )
        .await?;

        Ok(EnsureDirectRoomResult { room_id, created: true })
    }

    pub async fn create_or_reuse_direct_message_room(
        &self,
        owner_user_id: &str,
        target_user_ids: &[String],
        config: FriendRoomCreateRoomConfig,
        actor_user_id: Option<&str>,
    ) -> ApiResult<EnsureDirectRoomResult> {
        if target_user_ids.len() == 1 {
            let result = self.ensure_direct_room(owner_user_id, &target_user_ids[0], config, actor_user_id).await?;
            self.upsert_direct_room_links(owner_user_id, target_user_ids, &result.room_id).await?;
            return Ok(result);
        }

        let response = self
            .room_service
            .lifecycle()
            .create_room(owner_user_id, config.into())
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;

        let room_id = response
            .get("room_id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ApiError::internal("Failed to get room_id from create_room response"))?
            .to_string();

        self.upsert_direct_room_links(owner_user_id, target_user_ids, &room_id).await?;

        Ok(EnsureDirectRoomResult { room_id, created: true })
    }

    pub async fn get_friends_page(&self, user_id: &str, request: FriendListRequest) -> ApiResult<FriendListPage> {
        let room_id = self.create_friend_list_room(user_id).await?;
        let content = self
            .friend_storage
            .get_friend_list_content(&room_id)
            .await
            .map_err(|e| ApiError::database_with_log("Database error", &e))?
            .unwrap_or_else(|| json!({ "friends": [], "version": 1 }));

        let version = content.get("version").and_then(|v| v.as_i64()).unwrap_or(1);
        let safe_limit = request.limit.clamp(1, 100);
        if let Some(cursor) = request.from.as_ref() {
            if cursor.sort_by != request.sort_by {
                return Err(ApiError::bad_request("Friend list cursor sort order does not match request"));
            }
        }
        let page_key = request.from.as_ref().map_or_else(
            || format!("offset:{}", request.offset.unwrap_or(0)),
            |cursor| format!("cursor:{}", encode_friend_list_cursor(cursor)),
        );
        let cache_key = format!("friends:list:v3:{}:{}:{}:{}:{}", user_id, room_id, version, request.sort_by, page_key);

        if let Ok(Some(mut cached)) = self.cache.get::<FriendListPage>(&cache_key).await {
            cached.cached = true;
            cached.limit = safe_limit;
            return Ok(cached);
        }

        let raw_friends = content.get("friends").and_then(|friends| friends.as_array()).cloned().unwrap_or_default();
        let friend_ids: Vec<String> = raw_friends
            .iter()
            .filter_map(|friend| friend.get("user_id").and_then(|value| value.as_str()).map(ToOwned::to_owned))
            .collect();
        let profiles = self
            .user_storage
            .get_user_profiles_map(&friend_ids)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to load friend profiles", &e))?;
        let presence_map = self
            .presence_storage
            .get_presence_snapshots(&friend_ids)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to load presence snapshots", &e))?;

        let mut items = Self::build_friend_entries(raw_friends, &profiles, &presence_map);
        Self::sort_friend_entries(&mut items, &request.sort_by);

        let total = items.len();
        let offset = request.offset.unwrap_or(0).min(total);
        let start_index = if let Some(cursor) = request.from.as_ref() {
            items
                .iter()
                .position(|item| {
                    Self::compare_friend_entry_to_cursor(item, cursor, &request.sort_by) == Ordering::Greater
                })
                .unwrap_or(total)
        } else {
            offset
        };
        let paged_items = items.iter().skip(start_index).take(safe_limit).cloned().collect::<Vec<_>>();
        let next_offset =
            request.from.is_none().then_some(start_index + paged_items.len()).filter(|next| *next < total);
        let next_batch = if start_index + paged_items.len() < total {
            paged_items
                .last()
                .map(|item| encode_friend_list_cursor(&Self::cursor_from_friend_entry(item, &request.sort_by)))
        } else {
            None
        };

        let page = FriendListPage {
            room_id,
            items: paged_items,
            total,
            limit: safe_limit,
            offset: request.from.is_none().then_some(start_index),
            next_offset,
            next_batch,
            version,
            cached: false,
            generated_ts: current_timestamp_millis(),
        };

        if let Err(e) = self.cache.set(&cache_key, page.clone(), FRIEND_LIST_CACHE_TTL_SECS).await {
            ::tracing::warn!(
                user_id = %user_id,
                cache_key = %cache_key,
                limit = safe_limit,
                offset = start_index,
                error = %e,
                "Failed to cache friend list page"
            );
        }

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
            .map_err(|e| ApiError::database_with_log("Failed to load friend DM links", &e))?;

        if links.is_empty() {
            return Ok(0);
        }

        let now = current_timestamp_millis();
        let mut updated_lists = 0usize;

        for link in links {
            let mut content = link.content;
            let mut touched = false;

            if let Some(friends) = content.get_mut("friends").and_then(|value| value.as_array_mut()) {
                for friend in friends.iter_mut() {
                    if friend.get("dm_room_id").and_then(|value| value.as_str()) != Some(dm_room_id) {
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

            self.send_state_event(&link.friend_room_id, &link.owner_user_id, "m.friends.list", "", content).await?;
            updated_lists += 1;
        }

        Ok(updated_lists)
    }

    /// 处理收到的好友请求 (Federation)
    pub async fn handle_incoming_friend_request(
        &self,
        user_id: &str,
        requester_id: &str,
        content: serde_json::Value,
    ) -> ApiResult<()> {
        let message = content.get("message").and_then(|m| m.as_str());

        self.friend_storage.create_friend_request_with_user_ensure(requester_id, user_id, message).await.map_err(
            |e| {
                let error_msg = e.to_string();
                if error_msg.contains("foreign key") {
                    ApiError::database_with_log("Failed to create friend request: user not found", &error_msg)
                } else {
                    ApiError::database_with_log("Failed to create friend request", &error_msg)
                }
            },
        )?;

        Ok(())
    }

    // --- Helpers ---

    pub(crate) fn is_remote_user(&self, user_id: &str) -> bool {
        !user_id.ends_with(&format!(":{}", self.server_name))
    }

    pub(crate) async fn send_state_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
        state_key: &str,
        content: serde_json::Value,
    ) -> ApiResult<()> {
        let now = current_timestamp_millis();
        self.room_service
            .messaging()
            .create_event(
                CreateEventParams {
                    event_id: generate_event_id(&self.server_name),
                    room_id: room_id.to_string(),
                    user_id: user_id.to_string(),
                    event_type: event_type.to_string(),
                    content,
                    state_key: Some(state_key.to_string()),
                    origin_server_ts: now,
                    redacts: None,
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
            .map_err(|e| ApiError::database_with_log("Database error", &e))?
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
                    "added_at": current_timestamp_millis(),
                    "dm_room_id": dm_room_id,
                    "dm_room_active": dm_room_id.is_some(),
                    "dm_room_state": if dm_room_id.is_some() { "active" } else { "none" },
                    "dm_room_updated_ts": current_timestamp_millis()
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

    async fn update_existing_friend_dm_link(
        &self,
        owner_user_id: &str,
        friend_id: &str,
        dm_room_id: &str,
        dm_room_state: &str,
        changed_by: Option<&str>,
        reason: Option<&str>,
    ) -> ApiResult<bool> {
        let Some(friend_room_id) = self
            .friend_storage
            .get_friend_list_room_id(owner_user_id)
            .await
            .map_err(|e| ApiError::database_with_log("Database error", &e))?
        else {
            return Ok(false);
        };

        let mut content = self
            .friend_storage
            .get_friend_list_content(&friend_room_id)
            .await
            .map_err(|e| ApiError::database_with_log("Database error", &e))?
            .unwrap_or_else(|| json!({ "friends": [], "version": 1 }));

        let now = current_timestamp_millis();
        let mut touched = false;

        if let Some(friends) = content.get_mut("friends").and_then(|value| value.as_array_mut()) {
            for friend in friends.iter_mut() {
                if friend.get("user_id").and_then(|value| value.as_str()) != Some(friend_id) {
                    continue;
                }

                friend["dm_room_id"] = json!(dm_room_id);
                friend["dm_room_state"] = json!(dm_room_state);
                friend["dm_room_active"] = json!(dm_room_state == "active");
                friend["dm_room_updated_ts"] = json!(now);

                if let Some(changed_by) = changed_by {
                    friend["dm_room_changed_by"] = json!(changed_by);
                }

                if let Some(reason) = reason {
                    friend["dm_room_reason"] = json!(reason);
                }

                touched = true;
                break;
            }
        }

        if !touched {
            return Ok(false);
        }

        if let Some(version) = content.get("version").and_then(|value| value.as_i64()) {
            content["version"] = json!(version + 1);
        }

        self.send_state_event(&friend_room_id, owner_user_id, "m.friends.list", "", content).await?;

        Ok(true)
    }

    async fn create_friend_dm_room(&self, user_id: &str, friend_id: &str) -> ApiResult<String> {
        let config = FriendRoomCreateRoomConfig {
            visibility: Some("private".to_string()),
            preset: Some("trusted_private_chat".to_string()),
            invite_list: Some(vec![friend_id.to_string()]),
            is_direct: Some(true),
            ..Default::default()
        };

        self.ensure_direct_room(user_id, friend_id, config, Some(user_id)).await.map(|result| result.room_id)
    }

    pub(crate) fn build_friend_entries(
        raw_friends: Vec<serde_json::Value>,
        profiles: &HashMap<String, synapse_storage::UserProfile>,
        presence_map: &HashMap<String, synapse_storage::presence::PresenceSnapshot>,
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
                let fallback_name = displayname.clone().or(username.clone()).unwrap_or_else(|| user_id.clone());
                let presence = presence_map
                    .get(&user_id)
                    .map_or_else(|| "offline".to_string(), |snapshot| snapshot.presence.clone());
                let last_active_ts = presence_map.get(&user_id).and_then(|snapshot| snapshot.last_active_ts);

                Some(FriendListEntry {
                    user_id,
                    username,
                    display_name: displayname,
                    avatar_url: profile.and_then(|value| value.avatar_url.clone()),
                    note: friend.get("note").and_then(|value| value.as_str()).map(ToOwned::to_owned),
                    status: friend.get("status").and_then(|value| value.as_str()).unwrap_or("normal").to_string(),
                    online: presence == "online",
                    presence,
                    last_active_ts,
                    last_seen_ts: last_active_ts,
                    added_ts: friend.get("added_at").and_then(|value| value.as_i64()),
                    sort_letter: sort_letter_for(&fallback_name),
                    dm_room_id: friend.get("dm_room_id").and_then(|value| value.as_str()).map(ToOwned::to_owned),
                    dm_room_active: friend
                        .get("dm_room_active")
                        .and_then(|value| value.as_bool())
                        .unwrap_or_else(|| friend.get("dm_room_id").is_some()),
                    dm_room_state: friend.get("dm_room_state").and_then(|value| value.as_str()).map(ToOwned::to_owned),
                    dm_room_updated_ts: friend.get("dm_room_updated_ts").and_then(|value| value.as_i64()),
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

    pub(crate) fn sort_friend_entries(items: &mut [FriendListEntry], sort_by: &str) {
        items.sort_by(|left, right| Self::compare_friend_entries(left, right, sort_by));
    }

    pub(crate) fn compare_friend_entries(left: &FriendListEntry, right: &FriendListEntry, sort_by: &str) -> Ordering {
        match sort_by {
            "activity" => right
                .online
                .cmp(&left.online)
                .then_with(|| right.last_active_ts.cmp(&left.last_active_ts))
                .then_with(|| right.added_ts.cmp(&left.added_ts))
                .then_with(|| left.user_id.cmp(&right.user_id)),
            "recent" => right
                .added_ts
                .cmp(&left.added_ts)
                .then_with(|| right.last_active_ts.cmp(&left.last_active_ts))
                .then_with(|| left.user_id.cmp(&right.user_id)),
            _ => left
                .sort_letter
                .cmp(&right.sort_letter)
                .then_with(|| Self::friend_display_key(left).cmp(Self::friend_display_key(right)))
                .then_with(|| left.user_id.cmp(&right.user_id)),
        }
    }

    pub(crate) fn compare_friend_entry_to_cursor(
        item: &FriendListEntry,
        cursor: &FriendListCursor,
        sort_by: &str,
    ) -> Ordering {
        match sort_by {
            "activity" => cursor
                .online
                .cmp(&item.online)
                .then_with(|| cursor.last_active_ts.cmp(&item.last_active_ts))
                .then_with(|| cursor.added_ts.cmp(&item.added_ts))
                .then_with(|| item.user_id.cmp(&cursor.user_id)),
            "recent" => cursor
                .added_ts
                .cmp(&item.added_ts)
                .then_with(|| cursor.last_active_ts.cmp(&item.last_active_ts))
                .then_with(|| item.user_id.cmp(&cursor.user_id)),
            _ => item
                .sort_letter
                .cmp(&cursor.sort_letter)
                .then_with(|| Self::friend_display_key(item).cmp(cursor.display_key.as_str()))
                .then_with(|| item.user_id.cmp(&cursor.user_id)),
        }
    }

    pub(crate) fn cursor_from_friend_entry(item: &FriendListEntry, sort_by: &str) -> FriendListCursor {
        FriendListCursor {
            sort_by: sort_by.to_string(),
            sort_letter: item.sort_letter.clone(),
            display_key: Self::friend_display_key(item).to_string(),
            online: item.online,
            last_active_ts: item.last_active_ts,
            added_ts: item.added_ts,
            user_id: item.user_id.clone(),
        }
    }

    pub(crate) fn friend_display_key(item: &FriendListEntry) -> &str {
        item.display_name.as_deref().or(item.username.as_deref()).unwrap_or(item.user_id.as_str())
    }

    pub(crate) fn build_direct_room_snapshot(direct_map: Map<String, Value>, room_id: &str) -> DirectRoomSnapshot {
        let users = get_room_direct_users(&direct_map, room_id);
        let is_direct = !users.is_empty();

        DirectRoomSnapshot { direct_map, users, is_direct }
    }
}

#[async_trait::async_trait]
impl FriendRoomProvider for FriendRoomService {
    async fn handle_incoming_friend_request(
        &self,
        user_id: &str,
        requester_id: &str,
        content: serde_json::Value,
    ) -> Result<(), ApiError> {
        self.handle_incoming_friend_request(user_id, requester_id, content).await
    }
}

#[cfg(test)]
mod tests {
    use super::models::{FriendListCursor, FriendListEntry};
    use super::FriendRoomService;
    use crate::ServiceContainer;
    use serde_json::{json, Map, Value};
    use std::cmp::Ordering;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
    use std::sync::Arc;
    use synapse_cache::{CacheConfig, CacheManager};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    /// Generate a globally unique suffix across test binary runs by combining:
    /// - process PID (isolates concurrent test runs)
    /// - nanosecond timestamp (isolates sequential test runs)
    /// - monotonic counter (isolates calls within a single test run)
    fn unique_suffix() -> u64 {
        let counter = TEST_COUNTER.fetch_add(1, AtomicOrdering::SeqCst);
        let pid = std::process::id() as u64;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        // Mix all three components to ensure uniqueness across runs
        nanos.wrapping_mul(31).wrapping_add(pid << 16).wrapping_add(counter)
    }

    async fn setup_test_container() -> Option<ServiceContainer> {
        // Use isolated schema (each test gets its own fully-migrated schema)
        // to avoid cross-test pollution from the shared template, which may
        // accumulate leftover users/devices across test runs.
        let pool = match crate::test_utils::prepare_isolated_test_pool().await {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!(
                    "Isolated schema setup failed for friend room service tests ({error}); retrying with shared schema"
                );
                match crate::test_utils::prepare_shared_test_pool().await {
                    Ok(pool) => pool,
                    Err(error) => {
                        eprintln!("Skipping friend room service tests because test database is unavailable: {error}");
                        return None;
                    }
                }
            }
        };

        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        Some(ServiceContainer::new_test_with_pool_and_cache(pool, cache).await)
    }

    async fn register_test_user(container: &ServiceContainer, username: &str, display_name: &str) -> String {
        let (user, _, _, _) = container
            .core
            .credential_auth
            .register(username, "Test@123", false, Some(display_name))
            .await
            .expect("register test user");
        user.user_id
    }

    async fn establish_friendship(container: &ServiceContainer, alice_user_id: &str, bob_user_id: &str) {
        container
            .extensions
            .friend_room_service
            .send_friend_request("test-request-id", alice_user_id, bob_user_id, Some("hello"))
            .await
            .expect("send friend request");
        container
            .extensions
            .friend_room_service
            .accept_friend_request("test-request-id", bob_user_id, alice_user_id)
            .await
            .expect("accept friend request");
    }

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

    // ── friend_display_key ──────────────────────────────────────────

    fn make_entry(display_name: Option<&str>, username: Option<&str>, user_id: &str) -> FriendListEntry {
        FriendListEntry {
            user_id: user_id.to_string(),
            username: username.map(Into::into),
            display_name: display_name.map(Into::into),
            avatar_url: None,
            note: None,
            status: "normal".to_string(),
            online: false,
            presence: "offline".to_string(),
            last_active_ts: None,
            last_seen_ts: None,
            added_ts: None,
            sort_letter: "A".to_string(),
            dm_room_id: None,
            dm_room_active: false,
            dm_room_state: None,
            dm_room_updated_ts: None,
            dm_room_affected_user_id: None,
            dm_room_changed_by: None,
            dm_room_reason: None,
        }
    }

    #[test]
    fn friend_display_key_prefers_display_name() {
        let entry = make_entry(Some("Alice"), Some("alice99"), "@alice:example.com");
        assert_eq!(FriendRoomService::friend_display_key(&entry), "Alice");
    }

    #[test]
    fn friend_display_key_falls_back_to_username() {
        let entry = make_entry(None, Some("bob_cat"), "@bob:example.com");
        assert_eq!(FriendRoomService::friend_display_key(&entry), "bob_cat");
    }

    #[test]
    fn friend_display_key_falls_back_to_user_id() {
        let entry = make_entry(None, None, "@carol:example.com");
        assert_eq!(FriendRoomService::friend_display_key(&entry), "@carol:example.com");
    }

    #[test]
    fn friend_display_key_with_empty_strings() {
        let entry = FriendListEntry {
            display_name: Some("".to_string()),
            username: Some("".to_string()),
            ..make_entry(None, None, "@dave:example.com")
        };
        // "" is Some("") so should be returned
        assert_eq!(FriendRoomService::friend_display_key(&entry), "");
    }

    // ── compare_friend_entries ───────────────────────────────────────

    #[test]
    fn compare_friend_entries_activity_online_first() {
        let online =
            FriendListEntry { online: true, last_active_ts: Some(1000), ..make_entry(None, None, "@a:ex.com") };
        let offline =
            FriendListEntry { online: false, last_active_ts: Some(2000), ..make_entry(None, None, "@b:ex.com") };
        assert_eq!(FriendRoomService::compare_friend_entries(&online, &offline, "activity"), Ordering::Less);
        // online < offline means online comes first
    }

    #[test]
    fn compare_friend_entries_activity_by_last_active() {
        let recent = FriendListEntry {
            online: true,
            last_active_ts: Some(2000),
            added_ts: None,
            ..make_entry(None, None, "@a:ex.com")
        };
        let older = FriendListEntry {
            online: true,
            last_active_ts: Some(1000),
            added_ts: None,
            ..make_entry(None, None, "@b:ex.com")
        };
        assert_eq!(FriendRoomService::compare_friend_entries(&recent, &older, "activity"), Ordering::Less);
    }

    #[test]
    fn compare_friend_entries_recent_by_added_ts() {
        let newer =
            FriendListEntry { added_ts: Some(2000), last_active_ts: None, ..make_entry(None, None, "@a:ex.com") };
        let older =
            FriendListEntry { added_ts: Some(1000), last_active_ts: None, ..make_entry(None, None, "@b:ex.com") };
        assert_eq!(FriendRoomService::compare_friend_entries(&newer, &older, "recent"), Ordering::Less);
    }

    #[test]
    fn compare_friend_entries_alphabet_by_sort_letter() {
        let a = FriendListEntry {
            sort_letter: "A".into(),
            display_name: Some("Alice".into()),
            ..make_entry(None, None, "@a:ex.com")
        };
        let b = FriendListEntry {
            sort_letter: "B".into(),
            display_name: Some("Bob".into()),
            ..make_entry(None, None, "@b:ex.com")
        };
        assert_eq!(FriendRoomService::compare_friend_entries(&a, &b, "alphabet"), Ordering::Less);
    }

    #[test]
    fn compare_friend_entries_alphabet_same_letter_falls_back_to_display_key() {
        let a = FriendListEntry {
            sort_letter: "A".into(),
            display_name: Some("Alice".into()),
            ..make_entry(None, None, "@a:ex.com")
        };
        let b = FriendListEntry {
            sort_letter: "A".into(),
            display_name: Some("Bob".into()),
            ..make_entry(None, None, "@b:ex.com")
        };
        assert_eq!(FriendRoomService::compare_friend_entries(&a, &b, "alphabet"), Ordering::Less);
    }

    #[test]
    fn compare_friend_entries_alphabet_same_falls_back_to_user_id() {
        let a = FriendListEntry {
            sort_letter: "A".into(),
            display_name: Some("Same".into()),
            user_id: "@a:ex.com".into(),
            ..make_entry(None, None, "@a:ex.com")
        };
        let b = FriendListEntry {
            sort_letter: "A".into(),
            display_name: Some("Same".into()),
            user_id: "@b:ex.com".into(),
            ..make_entry(None, None, "@b:ex.com")
        };
        assert_eq!(FriendRoomService::compare_friend_entries(&a, &b, "alphabet"), Ordering::Less);
    }

    #[test]
    fn compare_friend_entries_unknown_sort_defaults_to_alphabet() {
        let a = FriendListEntry {
            sort_letter: "A".into(),
            display_name: Some("Alice".into()),
            ..make_entry(None, None, "@a:ex.com")
        };
        let b = FriendListEntry {
            sort_letter: "Z".into(),
            display_name: Some("Zoe".into()),
            ..make_entry(None, None, "@z:ex.com")
        };
        assert_eq!(FriendRoomService::compare_friend_entries(&a, &b, "unknown"), Ordering::Less);
    }

    // ── sort_friend_entries ──────────────────────────────────────────

    #[test]
    fn sort_friend_entries_by_alphabet() {
        let b = FriendListEntry {
            sort_letter: "B".into(),
            display_name: Some("Bob".into()),
            ..make_entry(None, None, "@b:ex.com")
        };
        let a = FriendListEntry {
            sort_letter: "A".into(),
            display_name: Some("Alice".into()),
            ..make_entry(None, None, "@a:ex.com")
        };
        let mut items = vec![b.clone(), a.clone()];
        FriendRoomService::sort_friend_entries(&mut items, "alphabet");
        assert_eq!(items[0].user_id, a.user_id);
        assert_eq!(items[1].user_id, b.user_id);
    }

    // ── cursor_from_friend_entry ─────────────────────────────────────

    #[test]
    fn cursor_from_friend_entry_captures_all_fields() {
        let entry = FriendListEntry {
            sort_letter: "C".into(),
            display_name: Some("Carol".into()),
            online: true,
            last_active_ts: Some(1700000000000),
            added_ts: Some(1690000000000),
            user_id: "@carol:ex.com".into(),
            ..make_entry(None, None, "@carol:ex.com")
        };
        let cursor = FriendRoomService::cursor_from_friend_entry(&entry, "activity");
        assert_eq!(cursor.sort_by, "activity");
        assert_eq!(cursor.sort_letter, "C");
        assert_eq!(cursor.display_key, "Carol");
        assert!(cursor.online);
        assert_eq!(cursor.last_active_ts, Some(1700000000000));
        assert_eq!(cursor.added_ts, Some(1690000000000));
        assert_eq!(cursor.user_id, "@carol:ex.com");
    }

    // ── compare_friend_entry_to_cursor ───────────────────────────────

    #[test]
    fn compare_entry_to_cursor_activity() {
        let entry =
            FriendListEntry { online: false, last_active_ts: Some(1000), ..make_entry(None, None, "@b:ex.com") };
        let cursor = FriendListCursor {
            sort_by: "activity".into(),
            sort_letter: "".into(),
            display_key: "".into(),
            online: true,
            last_active_ts: Some(2000),
            added_ts: None,
            user_id: "@a:ex.com".into(),
        };
        // cursor.online(true) > item.online(false) → cursor > item → item after cursor
        assert_eq!(FriendRoomService::compare_friend_entry_to_cursor(&entry, &cursor, "activity"), Ordering::Greater);
    }

    #[test]
    fn compare_entry_to_cursor_recent() {
        let entry =
            FriendListEntry { added_ts: Some(1000), last_active_ts: None, ..make_entry(None, None, "@b:ex.com") };
        let cursor = FriendListCursor {
            sort_by: "recent".into(),
            sort_letter: "".into(),
            display_key: "".into(),
            online: false,
            last_active_ts: None,
            added_ts: Some(2000),
            user_id: "@a:ex.com".into(),
        };
        // cursor (2000) > item (1000) → item after cursor
        assert_eq!(FriendRoomService::compare_friend_entry_to_cursor(&entry, &cursor, "recent"), Ordering::Greater);
    }

    #[test]
    fn compare_entry_to_cursor_alphabet() {
        let entry = FriendListEntry {
            sort_letter: "B".into(),
            display_name: Some("Bob".into()),
            user_id: "@b:ex.com".into(),
            ..make_entry(None, None, "@b:ex.com")
        };
        let cursor = FriendListCursor {
            sort_by: "alphabet".into(),
            sort_letter: "A".into(),
            display_key: "Alice".into(),
            online: false,
            last_active_ts: None,
            added_ts: None,
            user_id: "@a:ex.com".into(),
        };
        assert_eq!(FriendRoomService::compare_friend_entry_to_cursor(&entry, &cursor, "alphabet"), Ordering::Greater);
    }

    #[test]
    fn compare_entry_to_cursor_same_letter_falls_back_to_display_key() {
        let entry = FriendListEntry {
            sort_letter: "A".into(),
            display_name: Some("Bob".into()),
            user_id: "@b:ex.com".into(),
            ..make_entry(None, None, "@b:ex.com")
        };
        let cursor = FriendListCursor {
            sort_by: "alphabet".into(),
            sort_letter: "A".into(),
            display_key: "Alice".into(),
            online: false,
            last_active_ts: None,
            added_ts: None,
            user_id: "@a:ex.com".into(),
        };
        assert_eq!(FriendRoomService::compare_friend_entry_to_cursor(&entry, &cursor, "alphabet"), Ordering::Greater);
    }

    // ── build_direct_room_snapshot ────────────────────────────────────

    #[test]
    fn build_direct_room_snapshot_with_users() {
        let direct_map =
            serde_json::from_str::<Map<String, Value>>(r#"{"@alice:ex.com":["!room1:ex.com","!room2:ex.com"]}"#)
                .unwrap();
        let snapshot = FriendRoomService::build_direct_room_snapshot(direct_map, "!room1:ex.com");
        assert!(snapshot.is_direct);
        assert_eq!(snapshot.users, vec!["@alice:ex.com"]);
    }

    #[test]
    fn build_direct_room_snapshot_empty() {
        let direct_map: Map<String, Value> = Map::new();
        let snapshot = FriendRoomService::build_direct_room_snapshot(direct_map, "!room1:ex.com");
        assert!(!snapshot.is_direct);
        assert!(snapshot.users.is_empty());
    }

    #[test]
    fn build_direct_room_snapshot_room_not_in_map() {
        let direct_map = serde_json::from_str::<Map<String, Value>>(r#"{"@alice:ex.com":["!other:ex.com"]}"#).unwrap();
        let snapshot = FriendRoomService::build_direct_room_snapshot(direct_map, "!room1:ex.com");
        assert!(!snapshot.is_direct);
        assert!(snapshot.users.is_empty());
    }

    // ── build_friend_entries ─────────────────────────────────────────

    #[test]
    fn build_friend_entries_from_raw_data() {
        let raw = vec![json!({
            "user_id": "@alice:ex.com",
            "displayname": "Alice",
            "note": "best friend",
            "status": "normal",
            "added_at": 1690000000000_i64,
            "dm_room_id": "!dm:ex.com",
            "dm_room_active": true,
            "dm_room_state": "invite",
            "dm_room_updated_ts": 1700000000000_i64,
            "dm_room_affected_user_id": "@bob:ex.com",
            "dm_room_changed_by": "@bob:ex.com",
            "dm_room_reason": "hello"
        })];
        let profiles = HashMap::new();
        let presence_map = HashMap::new();
        let entries = FriendRoomService::build_friend_entries(raw, &profiles, &presence_map);
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.user_id, "@alice:ex.com");
        assert_eq!(e.display_name.as_deref(), Some("Alice"));
        assert_eq!(e.note.as_deref(), Some("best friend"));
        assert_eq!(e.presence, "offline");
        assert_eq!(e.sort_letter, "A");
    }

    #[test]
    fn build_friend_entries_falls_back_to_profile() {
        let raw = vec![json!({"user_id": "@bob:ex.com"})];
        let mut profiles = HashMap::new();
        profiles.insert(
            "@bob:ex.com".to_string(),
            synapse_storage::UserProfile {
                user_id: "@bob:ex.com".to_string(),
                displayname: Some("Bob Display".to_string()),
                avatar_url: Some("mxc://ex.com/avatar".to_string()),
                username: "bob99".to_string(),
                created_ts: 0,
            },
        );
        let presence_map = HashMap::new();
        let entries = FriendRoomService::build_friend_entries(raw, &profiles, &presence_map);
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.display_name.as_deref(), Some("Bob Display"));
        assert_eq!(e.avatar_url.as_deref(), Some("mxc://ex.com/avatar"));
        assert_eq!(e.username.as_deref(), Some("bob99"));
    }

    #[test]
    fn build_friend_entries_with_presence() {
        let raw = vec![json!({"user_id": "@carol:ex.com"})];
        let profiles = HashMap::new();
        let mut presence_map = HashMap::new();
        presence_map.insert(
            "@carol:ex.com".to_string(),
            synapse_storage::presence::PresenceSnapshot {
                user_id: "@carol:ex.com".to_string(),
                presence: "online".to_string(),
                status_msg: None,
                last_active_ts: Some(1700000000000),
            },
        );
        let entries = FriendRoomService::build_friend_entries(raw, &profiles, &presence_map);
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert!(e.online);
        assert_eq!(e.presence, "online");
        assert_eq!(e.last_active_ts, Some(1700000000000));
    }

    #[test]
    fn build_friend_entries_skips_missing_user_id() {
        let raw = vec![json!({"note": "no user_id here"})];
        let profiles = HashMap::new();
        let presence_map = HashMap::new();
        let entries = FriendRoomService::build_friend_entries(raw, &profiles, &presence_map);
        assert!(entries.is_empty());
    }

    #[test]
    fn build_friend_entries_dm_room_active_defaults_true_when_dm_room_id_present() {
        let raw = vec![json!({"user_id": "@dave:ex.com", "dm_room_id": "!dm:ex.com"})];
        let profiles = HashMap::new();
        let presence_map = HashMap::new();
        let entries = FriendRoomService::build_friend_entries(raw, &profiles, &presence_map);
        assert!(entries[0].dm_room_active);
    }

    #[tokio::test]
    async fn test_get_existing_dm_room_id_returns_persisted_friend_dm() {
        let Some(container) = setup_test_container().await else {
            return;
        };

        let suffix = unique_suffix();
        let alice_user_id = register_test_user(&container, &format!("friendsvc_alice_{suffix}"), "Alice").await;
        let bob_user_id = register_test_user(&container, &format!("friendsvc_bob_{suffix}"), "Bob").await;

        establish_friendship(&container, &alice_user_id, &bob_user_id).await;

        let room_id = container
            .extensions
            .friend_room_service
            .get_existing_dm_room_id(&alice_user_id, &bob_user_id)
            .await
            .expect("query existing dm room");

        assert!(room_id.is_some());
        assert!(room_id.unwrap().starts_with('!'));
    }

    #[tokio::test]
    async fn test_get_dm_partner_for_room_returns_profile_info() {
        let Some(container) = setup_test_container().await else {
            return;
        };

        let suffix = unique_suffix();
        let alice_user_id = register_test_user(&container, &format!("friendsvc_partner_alice_{suffix}"), "Alice").await;
        let bob_user_id = register_test_user(&container, &format!("friendsvc_partner_bob_{suffix}"), "Bob").await;

        establish_friendship(&container, &alice_user_id, &bob_user_id).await;

        let room_id = container
            .extensions
            .friend_room_service
            .get_existing_dm_room_id(&alice_user_id, &bob_user_id)
            .await
            .expect("query existing dm room")
            .expect("existing dm room id");

        let partner = container
            .extensions
            .friend_room_service
            .get_dm_partner_for_room(&alice_user_id, &room_id)
            .await
            .expect("query dm partner")
            .expect("dm partner info");

        assert_eq!(partner.user_id, bob_user_id);
        assert_eq!(partner.display_name, "Bob");
    }
}

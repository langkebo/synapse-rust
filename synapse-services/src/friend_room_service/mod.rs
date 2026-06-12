pub mod groups;
pub mod models;
pub use models::*;

use crate::RoomService;
use serde_json::{json, Map, Value};
use sqlx::Row;
use std::collections::HashMap;
use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_common::traits::FriendRoomProvider;
use synapse_common::{generate_event_id, ApiError, ApiResult};
use synapse_federation::friend::FriendFederationClient;
use synapse_federation::KeyRotationManager;
use synapse_storage::{CreateEventParams, EventStorage, FriendRoomStorage, PresenceStorage, UserStorage};

const FRIEND_LIST_CACHE_TTL_SECS: u64 = 300;
const FRIEND_ROOM_ID_CACHE_TTL_SECS: u64 = 3600;

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
        let federation_client = Arc::new(FriendFederationClient::new(server_name.clone(), Some(key_rotation_manager)));
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
        // 先查 Redis 缓存
        let room_cache_key = format!("friends:room_id:{}", user_id);
        if let Ok(Some(room_id)) = self.cache.get::<String>(&room_cache_key).await {
            return Ok(room_id);
        }

        if let Ok(Some(room_id)) = self.friend_storage.get_friend_list_room_id(user_id).await {
            let _ = self.cache.set(&room_cache_key, room_id.clone(), FRIEND_ROOM_ID_CACHE_TTL_SECS).await;
            return Ok(room_id);
        }

        let config = crate::room_service::CreateRoomConfig {
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
        self.send_state_event(&room_id, user_id, "m.friends.list", "", content).await?;

        // 缓存新创建的 room_id
        let room_cache_key = format!("friends:room_id:{}", user_id);
        let _ = self.cache.set(&room_cache_key, room_id.clone(), FRIEND_ROOM_ID_CACHE_TTL_SECS).await;

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
                    "timestamp": chrono::Utc::now().timestamp_millis(),
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
                    "timestamp": chrono::Utc::now().timestamp_millis(),
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
                "timestamp": chrono::Utc::now().timestamp_millis(),
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
        let row = sqlx::query("SELECT content FROM account_data WHERE user_id = $1 AND data_type = 'm.direct'")
            .bind(user_id)
            .fetch_optional(&*self.user_storage.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to load m.direct account data", &e))?;

        match row {
            Some(row) => match row.get::<Option<Value>, _>("content") {
                Some(Value::Object(map)) => Ok(map),
                Some(_) => Err(ApiError::internal("Invalid m.direct account data format")),
                None => Ok(Map::new()),
            },
            None => Ok(Map::new()),
        }
    }

    pub async fn save_direct_map(&self, user_id: &str, direct_map: &Map<String, Value>) -> ApiResult<()> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r"
            INSERT INTO account_data (user_id, data_type, content, created_ts, updated_ts)
            VALUES ($1, 'm.direct', $2, $3, $3)
            ON CONFLICT (user_id, data_type) DO UPDATE
            SET content = EXCLUDED.content, updated_ts = EXCLUDED.updated_ts
            ",
        )
        .bind(user_id)
        .bind(Value::Object(direct_map.clone()))
        .bind(now)
        .execute(&*self.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to save m.direct account data", &e))?;

        Ok(())
    }

    pub async fn get_effective_direct_map(&self, user_id: &str) -> ApiResult<Map<String, Value>> {
        let mut direct_map = self.load_direct_map(user_id).await?;
        merge_direct_links(&mut direct_map, self.get_direct_message_links(user_id).await?);

        if direct_map.is_empty() {
            let rows = sqlx::query(
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
            .fetch_all(&*self.user_storage.pool)
            .await
            .map_err(|e| ApiError::database_with_log("Failed to build effective direct map", &e))?;

            for row in rows {
                ensure_room_in_direct_map(
                    &mut direct_map,
                    &row.get::<String, _>("other_user_id"),
                    &row.get::<String, _>("room_id"),
                );
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
        .fetch_optional(&*self.user_storage.pool)
        .await
        .map_err(|e| ApiError::database_with_log("Failed to query existing DM room", &e))?;

        Ok(row.map(|value| value.get::<String, _>("room_id")))
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

        let row = sqlx::query(
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
        .fetch_optional(&*self.user_storage.pool)
        .await
        .map_err(|e| ApiError::database_with_log("Failed to load DM partner from membership", &e))?;

        Ok(row.map(|row| DmPartnerInfo {
            user_id: row.get::<String, _>("user_id"),
            display_name: row.get::<String, _>("display_name"),
            avatar_url: row.get::<String, _>("avatar_url"),
        }))
    }

    pub async fn ensure_direct_room(
        &self,
        owner_user_id: &str,
        friend_user_id: &str,
        config: crate::room::service::CreateRoomConfig,
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
            .create_room(owner_user_id, config)
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
        config: crate::room::service::CreateRoomConfig,
        actor_user_id: Option<&str>,
    ) -> ApiResult<EnsureDirectRoomResult> {
        if target_user_ids.len() == 1 {
            let result = self.ensure_direct_room(owner_user_id, &target_user_ids[0], config, actor_user_id).await?;
            self.upsert_direct_room_links(owner_user_id, target_user_ids, &result.room_id).await?;
            return Ok(result);
        }

        let response = self
            .room_service
            .create_room(owner_user_id, config)
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
        let cache_key = format!(
            "friends:list:v2:{}:{}:{}:{}:{}:{}",
            user_id, room_id, version, request.sort_by, request.offset, safe_limit
        );

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
        let offset = request.offset.min(total);
        let paged_items = items.into_iter().skip(offset).take(safe_limit).collect::<Vec<_>>();
        let next_offset = (offset + paged_items.len() < total).then_some(offset + paged_items.len());

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

        let _ = self.cache.set(&cache_key, page.clone(), FRIEND_LIST_CACHE_TTL_SECS).await;

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

        let now = chrono::Utc::now().timestamp_millis();
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

    fn is_remote_user(&self, user_id: &str) -> bool {
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

        let now = chrono::Utc::now().timestamp_millis();
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
        let config = crate::room_service::CreateRoomConfig {
            visibility: Some("private".to_string()),
            preset: Some("trusted_private_chat".to_string()),
            invite_list: Some(vec![friend_id.to_string()]),
            is_direct: Some(true),
            ..Default::default()
        };

        self.ensure_direct_room(user_id, friend_id, config, Some(user_id)).await.map(|result| result.room_id)
    }

    fn build_friend_entries(
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
                        left.display_name.as_deref().or(left.username.as_deref()).unwrap_or(left.user_id.as_str()).cmp(
                            right
                                .display_name
                                .as_deref()
                                .or(right.username.as_deref())
                                .unwrap_or(right.user_id.as_str()),
                        )
                    })
                    .then_with(|| left.user_id.cmp(&right.user_id))
            }),
        }
    }

    fn build_direct_room_snapshot(direct_map: Map<String, Value>, room_id: &str) -> DirectRoomSnapshot {
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
    use crate::ServiceContainer;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use synapse_cache::{CacheConfig, CacheManager};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    fn unique_suffix() -> u64 {
        TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
    }

    async fn setup_test_container() -> Option<ServiceContainer> {
        let pool = match crate::test_utils::prepare_shared_test_pool().await {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!(
                    "Shared schema setup failed for friend room service tests ({error}); retrying with isolated schema"
                );
                match crate::test_utils::prepare_isolated_test_pool().await {
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
            .auth_service
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

/// The `groups` module.
pub mod groups;
/// The `models` module.
pub mod models;
/// The `sharding` module.
pub mod sharding;
use self::models::{
    ensure_room_in_direct_map, get_room_direct_users, merge_direct_links, remove_room_from_direct_map, sort_letter_for,
};
use self::sharding::{shard_for_user_id, shard_to_state_key};
pub use models::{
    decode_friend_list_cursor, encode_friend_list_cursor, DirectMapUpdateAction, DirectRoomSnapshot, DmPartnerInfo,
    EnsureDirectRoomResult, FriendListCursor, FriendListEntry, FriendListPage, FriendListRequest, FriendListSortCache,
    FriendRoomCreateRoomConfig, FriendRoomService,
};
use synapse_common::{current_timestamp_millis, generate_event_id, ApiError, ApiResult};

use crate::UserService;
use futures::future::try_join_all;
use serde_json::{json, Map, Value};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_common::traits::FriendRoomProvider;
use synapse_federation::friend::FriendFederationClient;
use synapse_federation::KeyRotationManager;
use synapse_storage::{CreateEventParams, UserStore};

const FRIEND_LIST_CACHE_TTL_SECS: u64 = 300;
const FRIEND_ROOM_ID_CACHE_TTL_SECS: u64 = 3600;

/// W5 sharding helper：把 fan-out 读取到的所有 shard content 合并成单一 Value。
///
/// 输入：fan-out 顺序的 `(state_key, content)` 列表（按 state_key 字典序）。
/// 输出：聚合后的 `{ "friends": [...], "version": N }`，其中：
/// - `friends[]` 拼接所有 shard 的 `friends` 数组（按字典序，避免分页边界跳变）
/// - `version` 取各 shard `version` 字段的 max —— 语义"任一 shard 变过 = 整体变过"
///
/// 兼容：当 `shards` 为空时返回默认空 content（与 v4 unwrap_or 行为一致）。
fn merge_friend_list_shards(shards: &[(String, Value)]) -> Value {
    if shards.is_empty() {
        return json!({ "friends": [], "version": 1 });
    }
    let mut all_friends: Vec<Value> = Vec::new();
    let mut max_version: i64 = 0;
    for (_state_key, content) in shards {
        if let Some(arr) = content.get("friends").and_then(|f| f.as_array()) {
            all_friends.extend(arr.iter().cloned());
        }
        if let Some(v) = content.get("version").and_then(|x| x.as_i64()) {
            if v > max_version {
                max_version = v;
            }
        }
    }
    // v4 老 data 可能 version=1 默认；空 shards 返回 1；这里至少 1 起步避免 0
    if max_version < 1 {
        max_version = 1;
    }
    json!({ "friends": all_friends, "version": max_version })
}

/// 读取「用于更新的好友列表 shard」，兼容 v4 legacy 数据（W5 sharding）。
///
/// v5 好友按 `shard_for_user_id(friend_id)` 路由到对应 `state_key` 的 shard；
/// 但 v4 时代的好友全写在 `state_key=""` 遗留通道，W5 不主动迁移。因此若目标
/// shard 不存在或不包含该 friend，回退到 legacy `""` shard 定位。
///
/// 返回 `(effective_state_key, content)`，调用方修改后写回 `effective_state_key` 即可
/// （新增好友写目标 shard；遗留好友就地写回 `""`，保持 no-migration 语义）。
pub(crate) async fn read_friend_shard_for_update(
    storage: &dyn synapse_storage::friend_room::FriendRoomStoreApi,
    room_id: &str,
    friend_id: &str,
) -> Result<(String, serde_json::Value), sqlx::Error> {
    let target = shard_to_state_key(shard_for_user_id(friend_id));
    if let Some(content) = storage.get_friend_list_shard(room_id, &target).await? {
        let present = content
            .get("friends")
            .and_then(|f| f.as_array())
            .map(|arr| arr.iter().any(|f| f.get("user_id").and_then(|u| u.as_str()) == Some(friend_id)))
            .unwrap_or(false);
        if present {
            return Ok((target, content));
        }
    }
    // v4 legacy 回退：升级前的好友可能仍在 state_key=""
    if let Some(content) = storage.get_friend_list_shard(room_id, "").await? {
        return Ok(("".to_string(), content));
    }
    Ok((target, json!({ "friends": [], "version": 1 })))
}

/// W6: cursor 翻页起点解析。
///
/// 输入：`items` 已排序的好友数组（cursor 翻页的目标数组）、
/// `request.from`（可选 cursor）和 `sort_by`。
/// 输出：`start_index: usize` —— cursor 翻页应跳过的 entry 数（unbounded 即 total）。
///
/// 算法：
/// 1. cursor 为 None → 用 `request.offset`（如有）或 0
/// 2. cursor 为 Some → 用 `partition_point` 二分查找首个
///    `compare_friend_entry_to_cursor(item, cursor) == Greater` 的位置（O(log n)）
///
/// W3 review 留下的 TODO 4 原本建议"二级缓存"消除 O(n) scan，但仔细分析后
/// `compare_friend_entry_to_cursor` 按 sort_by 决定的排序键是全序关系（`compare`
/// 走 Ord），`partition_point` 标准库二分天然成立：
/// - O(log n) vs 之前 O(n)：1000 好友 ~10 次比较 vs ~1000 次
/// - 0 cache 复杂度：不需要写回、不需要失效策略、不需要 Redis 反序列化额外字段
/// - cursor 数量无关：不像 cache 那样需要担心 BTreeMap 大小
///
/// `compare_friend_entry_to_cursor` 现有 `Greater` 含义"item 在 cursor 之后"，
/// 与本函数语义"返回比 cursor 严格更大（或靠后）的所有 entry"一致。
/// 谓词取反 `!= Greater` 等价于"≤ cursor"，partition_point 返回首个 true 位置
/// 即"第一个 > cursor"。
fn resolve_cursor_start_index(items: &[FriendListEntry], request: &FriendListRequest) -> usize {
    let Some(cursor) = request.from.as_ref() else {
        return request.offset.unwrap_or(0).min(items.len());
    };
    items.partition_point(|item| {
        FriendRoomService::compare_friend_entry_to_cursor(item, cursor, &request.sort_by) != Ordering::Greater
    })
}

impl FriendRoomService {
    /// See [`new`].
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

    /// See [`new_with_dependencies`].
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
    ///
    /// Uses a Redis SETNX distributed lock to prevent two concurrent requests
    /// from both passing the DB-miss check and calling `create_room()` twice.
    /// The lock key is `friend_room_lock:{user_id}` with a 5-second TTL so a
    /// crashed holder's lock auto-expires.
    ///
    /// If Redis is unavailable the lock is skipped (fail-open) — the DB's
    /// unique constraint on `m.direct` still protects against duplicate rows.
    pub async fn create_friend_list_room(&self, user_id: &str) -> ApiResult<String> {
        // Fast path: check Redis cache first
        let room_cache_key = format!("friends:room_id:{}", user_id);
        if let Ok(Some(room_id)) = self.cache.get::<String>(&room_cache_key).await {
            return Ok(room_id);
        }

        // Check DB
        if let Ok(Some(room_id)) = self.friend_storage.get_friend_list_room_id(user_id).await {
            let _ = self.cache.set(&room_cache_key, room_id.clone(), FRIEND_ROOM_ID_CACHE_TTL_SECS).await;
            return Ok(room_id);
        }

        // ── Race window: two requests can both see DB miss and call create_room().
        //    Protect it with a distributed lock.
        let lock_key = format!("friend_room_lock:{}", user_id);
        let lock_ttl = 5; // seconds

        let acquired = match self.cache.try_acquire_lock(&lock_key, lock_ttl).await {
            Ok(acquired) => acquired,
            Err(e) => {
                // Redis down — fail-open; DB unique constraint is the safety net
                tracing::warn!(user_id = %user_id, error = %e,
                    "Redis lock unavailable, proceeding without distributed lock");
                true
            }
        };

        if !acquired {
            // Another request is creating this room. Wait briefly then re-check.
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

            // Re-check cache
            if let Ok(Some(room_id)) = self.cache.get::<String>(&room_cache_key).await {
                return Ok(room_id);
            }
            // Re-check DB (room may have been created by the holder)
            if let Ok(Some(room_id)) = self.friend_storage.get_friend_list_room_id(user_id).await {
                let _ = self.cache.set(&room_cache_key, room_id.clone(), FRIEND_ROOM_ID_CACHE_TTL_SECS).await;
                return Ok(room_id);
            }
            tracing::warn!(user_id = %user_id,
                "Lock holder timed out, proceeding to create room");
        }

        // ── Lock acquired (or we decided to proceed after Redis failure) ──
        // Double-check DB inside lock in case the holder just finished
        if let Ok(Some(room_id)) = self.friend_storage.get_friend_list_room_id(user_id).await {
            let _ = self.cache.release_lock(&lock_key).await;
            let _ = self.cache.set(&room_cache_key, room_id.clone(), FRIEND_ROOM_ID_CACHE_TTL_SECS).await;
            return Ok(room_id);
        }

        tracing::debug!(user_id = %user_id, "Acquired friend-room lock, creating room");

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

        // Cache the newly created room_id
        let _ = self.cache.set(&room_cache_key, room_id.clone(), FRIEND_ROOM_ID_CACHE_TTL_SECS).await;

        // Always release the lock, even on panic (via Drop guard)
        self.cache.release_lock(&lock_key).await;

        tracing::debug!(user_id = %user_id, room_id = %room_id, "Friend room created successfully");
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
            .map_err(|e| ApiError::database_with_context("Failed to check friendship", &e))?
        {
            return Err(ApiError::conflict(format!("User {receiver_id} is already your friend")));
        }

        if self
            .friend_storage
            .has_any_pending_request(sender_id, receiver_id)
            .await
            .map_err(|e| ApiError::database_with_context("Failed to check pending request", &e))?
        {
            // Idempotent: return the existing pending request instead of 409
            if let Some(existing) = self
                .friend_storage
                .get_pending_friend_request(sender_id, receiver_id)
                .await
                .map_err(|e| ApiError::database_with_context("Failed to get existing request", &e))?
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
                .map_err(|e| ApiError::database_with_context("Failed to get existing reverse request", &e))?
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
                        ApiError::database_with_context("Failed to create friend request", &error_msg)
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
    ///
    /// 幂等设计：
    /// 1. 若双方已是好友 → 直接返回已有 DM 房间 ID（幂等成功）
    /// 2. 若请求已被接受过 → 标记为 accepted 并返回 DM 房间（幂等修复）
    /// 3. 若请求不存在 → 返回 404
    /// 4. 正常 pending 请求 → 执行完整 accept 流程
    #[::tracing::instrument(skip(self), fields(request_id = %request_id))]
    pub async fn accept_friend_request(
        &self,
        request_id: &str,
        user_id: &str,
        requester_id: &str,
    ) -> ApiResult<String> {
        // --- 幂等检查 1：双方已是好友，直接返回已有 DM 房间 ---
        let user_friend_room = self.create_friend_list_room(user_id).await?;
        if self
            .friend_storage
            .is_friend(&user_friend_room, requester_id)
            .await
            .map_err(|e| ApiError::database_with_context("Failed to check friendship", &e))?
        {
            tracing::info!(
                %request_id,
                user_id = %user_id,
                requester_id = %requester_id,
                "Accept skipped: already friends, returning existing DM room"
            );
            if let Some(dm_room_id) = self.get_existing_dm_room_id(user_id, requester_id).await? {
                // Defensive join: ensure both users are joined to the DM room.
                // Covers rooms created before P0 fix where acceptor has invite-only membership.
                for uid in [&user_id, &requester_id] {
                    if let Err(e) = self.room_service.membership().join_room(&dm_room_id, uid).await {
                        tracing::warn!(
                            user_id = %uid,
                            dm_room_id = %dm_room_id,
                            error = %e,
                            "accept_friend_request: failed to join DM room (non-fatal)"
                        );
                    }
                }
                return Ok(dm_room_id);
            }
            // 已是好友但找不到 DM 房间（数据不一致），继续执行创建流程
        }

        // --- 查找 pending 请求 ---
        let pending_request = self
            .friend_storage
            .get_pending_friend_request(requester_id, user_id)
            .await
            .map_err(|e| ApiError::database_with_context("Failed to get friend request", &e))?;

        if let Some(_request) = pending_request {
            // 正常 pending 请求，执行完整 accept 流程
            return self.execute_accept_flow(request_id, user_id, requester_id, &user_friend_room).await;
        }

        // --- 幂等检查 2：请求非 pending，检查是否已被接受过 ---
        let existing_request = self
            .friend_storage
            .get_friend_request(requester_id, user_id)
            .await
            .map_err(|e| ApiError::database_with_context("Failed to get friend request", &e))?;

        if let Some(ref request) = existing_request {
            if request.status == "accepted" {
                // 请求已被接受过，确保好友关系和 DM 房间存在
                tracing::info!(
                    %request_id,
                    user_id = %user_id,
                    requester_id = %requester_id,
                    "Accept skipped: request already accepted, ensuring friend state"
                );
                return self.ensure_accept_state(request_id, user_id, requester_id, &user_friend_room).await;
            }
            // 请求存在但状态是 rejected/cancelled，返回 409
            return Err(ApiError::conflict(format!(
                "Friend request from {requester_id} has been {request_status}",
                request_status = request.status
            )));
        }

        // --- 请求完全不存在，返回 404 ---
        Err(ApiError::not_found(format!("No friend request from {requester_id}")))
    }

    /// 执行完整的 accept 流程（创建 DM、更新好友列表、标记请求状态）
    async fn execute_accept_flow(
        &self,
        request_id: &str,
        user_id: &str,
        requester_id: &str,
        user_friend_room: &str,
    ) -> ApiResult<String> {
        let dm_room_id = self.create_friend_dm_room(user_id, requester_id).await?;

        // P0 fix: ensure_direct_room 可能返回已存在的 DM 房间（对方已创建并
        // invite 了本方），此时本方的 membership 是 invite 而非 join。显式 join
        // 幂等——已 join 则无操作。若不 join，所有房间操作（发消息/typing/
        // unread_count）均 403。
        if let Err(e) = self.room_service.membership().join_room(&dm_room_id, user_id).await {
            tracing::warn!(
                user_id = %user_id,
                dm_room_id = %dm_room_id,
                error = %e,
                "Failed to join DM room during friend accept (non-fatal)"
            );
        }

        let requester_friend_room = self.create_friend_list_room(requester_id).await?;

        self.update_friend_list(user_id, user_friend_room, requester_id, "add", Some(&dm_room_id)).await?;
        self.update_friend_list(requester_id, &requester_friend_room, user_id, "add", Some(&dm_room_id)).await?;

        self.friend_storage
            .update_friend_request_status(requester_id, user_id, "accepted")
            .await
            .map_err(|e| ApiError::database_with_context("Failed to update request status", &e))?;

        self.presence_storage
            .add_subscription(user_id, requester_id)
            .await
            .map_err(|e| ApiError::database_with_context("Failed to subscribe to presence", &e))?;
        self.presence_storage
            .add_subscription(requester_id, user_id)
            .await
            .map_err(|e| ApiError::database_with_context("Failed to subscribe to presence", &e))?;

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

    /// 幂等修复：请求已被 accept 过但好友状态可能不完整时，补齐关系和房间
    async fn ensure_accept_state(
        &self,
        request_id: &str,
        user_id: &str,
        requester_id: &str,
        user_friend_room: &str,
    ) -> ApiResult<String> {
        // 确保 DM 房间存在
        let dm_room_id = self.create_friend_dm_room(user_id, requester_id).await?;

        // Ensure the user joins the DM room (same as execute_accept_flow).
        if let Err(e) = self.room_service.membership().join_room(&dm_room_id, user_id).await {
            tracing::warn!(
                user_id = %user_id,
                dm_room_id = %dm_room_id,
                error = %e,
                "Failed to join DM room during ensure_accept_state (non-fatal)"
            );
        }

        // 确保双方好友列表中包含对方
        let requester_friend_room = self.create_friend_list_room(requester_id).await?;

        self.update_friend_list(user_id, user_friend_room, requester_id, "add", Some(&dm_room_id)).await?;
        self.update_friend_list(requester_id, &requester_friend_room, user_id, "add", Some(&dm_room_id)).await?;

        // 确保 presence 订阅
        let _ = self.presence_storage.add_subscription(user_id, requester_id).await;
        let _ = self.presence_storage.add_subscription(requester_id, user_id).await;

        tracing::info!(
            %request_id,
            user_id = %user_id,
            requester_id = %requester_id,
            dm_room_id = %dm_room_id,
            "Accept state ensured for already-accepted request"
        );

        Ok(dm_room_id)
    }

    /// 拒绝好友请求
    #[::tracing::instrument(skip(self), fields(request_id = %request_id))]
    pub async fn reject_friend_request(&self, request_id: &str, user_id: &str, requester_id: &str) -> ApiResult<()> {
        let updated = self
            .friend_storage
            .update_friend_request_status(requester_id, user_id, "rejected")
            .await
            .map_err(|e| ApiError::database_with_context("Failed to reject friend request", &e))?;

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
            .map_err(|e| ApiError::database_with_context("Failed to cancel friend request", &e))?;

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
            .map_err(|e| ApiError::database_with_context("Database error", &e))?;

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
            .map_err(|e| ApiError::database_with_context("Database error", &e))?;

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
            .map_err(|e| ApiError::database_with_context("Failed to check friendship", &e))?
        {
            return Err(ApiError::conflict(format!("User {friend_id} is already your friend")));
        }

        let dm_room_id = self.create_friend_dm_room(user_id, friend_id).await?;

        // Ensure the adding user joins the DM room (same as execute_accept_flow).
        if let Err(e) = self.room_service.membership().join_room(&dm_room_id, user_id).await {
            tracing::warn!(
                user_id = %user_id,
                dm_room_id = %dm_room_id,
                error = %e,
                "Failed to join DM room during add_friend (non-fatal)"
            );
        }

        self.update_friend_list(user_id, &user_friend_room, friend_id, "add", Some(&dm_room_id)).await?;

        self.presence_storage
            .add_subscription(user_id, friend_id)
            .await
            .map_err(|e| ApiError::database_with_context("Failed to subscribe to presence", &e))?;

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
            .map_err(|e| ApiError::database_with_context("Failed to check friendship", &e))?
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
            .map_err(|e| ApiError::database_with_context("Database error", &e))?
        else {
            return Ok(Vec::new());
        };

        let content = self
            .friend_storage
            .get_friend_list_all_shards(&room_id)
            .await
            .map_err(|e| ApiError::database_with_context("Database error", &e))?;
        let content = merge_friend_list_shards(&content);

        let links = content
            .get("friends")
            .and_then(|value| value.as_array())
            .map(|arr| arr.as_slice())
            .unwrap_or(&[])
            .iter()
            .filter_map(|friend| {
                let friend_id = friend.get("user_id").and_then(|value| value.as_str())?;
                let dm_room_id = friend.get("dm_room_id").and_then(|value| value.as_str())?;
                let is_active = friend.get("dm_room_active").and_then(|value| value.as_bool()).unwrap_or(true);

                is_active.then(|| (friend_id.to_owned(), dm_room_id.to_owned()))
            })
            .collect();

        Ok(links)
    }

    /// See [`load_direct_map`].
    pub async fn load_direct_map(&self, user_id: &str) -> ApiResult<Map<String, Value>> {
        let content = self
            .account_data_storage
            .get_account_data_content(user_id, "m.direct")
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to load m.direct account data", &e))?;

        match content {
            Some(Value::Object(map)) => Ok(map),
            Some(_) => Err(ApiError::internal("Invalid m.direct account data format")),
            None => Ok(Map::new()),
        }
    }

    /// See [`save_direct_map`].
    pub async fn save_direct_map(&self, user_id: &str, direct_map: &Map<String, Value>) -> ApiResult<()> {
        self.account_data_storage
            .upsert_account_data(user_id, "m.direct", Value::Object(direct_map.clone()))
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to save m.direct account data", &e))?;

        // Invalidate the account-data cache for this user so the next /sync
        // will re-read the fresh m.direct data (OPT-015-b, audit 04 §5).
        let _ = self.cache.delete(&format!("account_data:{user_id}")).await;

        Ok(())
    }

    /// See [`get_effective_direct_map`].
    pub async fn get_effective_direct_map(&self, user_id: &str) -> ApiResult<Map<String, Value>> {
        let mut direct_map = self.load_direct_map(user_id).await?;
        merge_direct_links(&mut direct_map, self.get_direct_message_links(user_id).await?);

        if direct_map.is_empty() {
            let rows = self
                .friend_storage
                .get_effective_direct_links_fallback(user_id)
                .await
                .map_err(|e| ApiError::database_with_context("Failed to build effective direct map", &e))?;

            for row in rows {
                ensure_room_in_direct_map(&mut direct_map, &row.other_user_id, &row.room_id);
            }
        }

        Ok(direct_map)
    }

    /// See [`get_direct_room_snapshot`].
    pub async fn get_direct_room_snapshot(&self, user_id: &str, room_id: &str) -> ApiResult<DirectRoomSnapshot> {
        let direct_map = self.get_effective_direct_map(user_id).await?;
        Ok(Self::build_direct_room_snapshot(direct_map, room_id))
    }

    /// See [`upsert_direct_room_links`].
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

    /// See [`apply_direct_map_update`].
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

    /// See [`update_direct_room_snapshot`].
    pub async fn update_direct_room_snapshot(
        &self,
        user_id: &str,
        room_id: &str,
        action: DirectMapUpdateAction,
    ) -> ApiResult<DirectRoomSnapshot> {
        let direct_map = self.apply_direct_map_update(user_id, action).await?;
        Ok(Self::build_direct_room_snapshot(direct_map, room_id))
    }

    /// See [`replace_direct_room_targets`].
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

    /// See [`overwrite_direct_map`].
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
            .map_err(|e| ApiError::database_with_context("Failed to query existing DM room", &e))
    }

    /// See [`get_dm_partner_for_room`].
    pub async fn get_dm_partner_for_room(&self, user_id: &str, room_id: &str) -> ApiResult<Option<DmPartnerInfo>> {
        if let Some((partner_user_id, _)) =
            self.get_direct_message_links(user_id).await?.into_iter().find(|(_, dm_room_id)| dm_room_id == room_id)
        {
            if let Some(profile) = self
                .user_storage
                .get_user_profile(&partner_user_id)
                .await
                .map_err(|e| ApiError::database_with_context("Failed to load DM partner profile", &e))?
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
            .map_err(|e| ApiError::database_with_context("Failed to load DM partner from membership", &e))?;

        Ok(partner.map(|row| DmPartnerInfo {
            user_id: row.user_id,
            display_name: row.display_name,
            avatar_url: row.avatar_url,
        }))
    }

    /// See [`ensure_direct_room`].
    pub async fn ensure_direct_room(
        &self,
        owner_user_id: &str,
        friend_user_id: &str,
        config: FriendRoomCreateRoomConfig,
        actor_user_id: Option<&str>,
    ) -> ApiResult<EnsureDirectRoomResult> {
        if let Some(room_id) = self.get_existing_dm_room_id(owner_user_id, friend_user_id).await? {
            // Defensive join: if either user is only "invite" (not yet joined), auto-join.
            // This covers rooms created before the P0 fix and edge cases where the
            // acceptor's join event was lost. join_room is idempotent but does 3 DB
            // queries even in the no-op case, so we check membership first.
            for uid in [&owner_user_id, &friend_user_id] {
                let membership = self
                    .room_service
                    .membership()
                    .resolve_membership_from(&room_id, uid)
                    .await
                    .ok()
                    .and_then(|(m, _)| m);
                if membership != Some(synapse_common::Membership::Join) {
                    if let Err(e) = self.room_service.membership().join_room(&room_id, uid).await {
                        tracing::warn!(
                            user_id = %uid,
                            room_id = %room_id,
                            error = %e,
                            "ensure_direct_room: failed to join existing DM room (non-fatal)"
                        );
                    }
                }
            }

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

    /// See [`create_or_reuse_direct_message_room`].
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

    /// See [`get_friends_page`].
    pub async fn get_friends_page(&self, user_id: &str, request: FriendListRequest) -> ApiResult<FriendListPage> {
        let room_id = self.create_friend_list_room(user_id).await?;
        // W5 sharding：fan-out 读所有 shard，fan-in 合并成单一 content。
        // merge_friend_list_shards 返回的 version = max(各 shard version)，
        // 任何 shard 写一次都会让这个聚合 version +1，触发 sort_cache 失效。
        let shards = self
            .friend_storage
            .get_friend_list_all_shards(&room_id)
            .await
            .map_err(|e| ApiError::database_with_context("Database error", &e))?;
        let content = merge_friend_list_shards(&shards);

        let version = content.get("version").and_then(|v| v.as_i64()).unwrap_or(1);
        let safe_limit = request.limit.clamp(1, 100);
        if let Some(cursor) = request.from.as_ref() {
            if cursor.sort_by != request.sort_by {
                return Err(ApiError::bad_request("Friend list cursor sort order does not match request"));
            }
        }

        // W3+W5: 两层缓存 — 排序列表（per sort_by）+ 分页（per request）
        // 1) 排序缓存：不同 limit 共享同一排序结果，命中率提升 ~3x
        // 2) 分页在排序结果上即时应用，O(1) 取数
        // 缓存 key v5：相对 v4 增加了 shard fingerprint 防御 shard 数变更
        // 触发的缓存不一致（v4 → v5 升级期间老缓存自动失效，无需手动清理）
        // 用每个 shard 自己的 version（而非合并后的全局 max），否则非 max shard 更新时
        // 全局 max 不变 → 指纹不变 → 排序缓存不失效（见 W5 review Blocker 1）。
        let shard_fingerprint = shards
            .iter()
            .map(|(k, shard_content)| {
                let v = shard_content.get("version").and_then(|x| x.as_i64()).unwrap_or(0);
                format!("{}:{}", k, v)
            })
            .collect::<Vec<_>>()
            .join("|");
        let sort_cache_key = format!(
            "friends:list:v5:sort:{}:{}:{}:{}:{}",
            user_id, room_id, version, request.sort_by, shard_fingerprint
        );
        let mut sort_cache_hit = false;
        let sort_cache: FriendListSortCache = match self.cache.get::<FriendListSortCache>(&sort_cache_key).await {
            Ok(Some(cached)) => {
                sort_cache_hit = true;
                cached
            }
            _ => {
                let raw_friends =
                    content.get("friends").and_then(|friends| friends.as_array()).cloned().unwrap_or_default();
                let friend_ids: Vec<String> = raw_friends
                    .iter()
                    .filter_map(|friend| friend.get("user_id").and_then(|value| value.as_str()).map(ToOwned::to_owned))
                    .collect();
                let profiles = self
                    .user_storage
                    .get_user_profiles_map(&friend_ids)
                    .await
                    .map_err(|e| ApiError::database_with_context("Failed to load friend profiles", &e))?;
                let presence_map = self
                    .presence_storage
                    .get_presence_snapshots(&friend_ids)
                    .await
                    .map_err(|e| ApiError::database_with_context("Failed to load presence snapshots", &e))?;

                let mut items = Self::build_friend_entries(raw_friends, &profiles, &presence_map);
                Self::sort_friend_entries(&mut items, &request.sort_by);

                let sort_cache = FriendListSortCache {
                    version,
                    sort_by: request.sort_by.clone(),
                    total: items.len(),
                    items,
                    generated_ts: current_timestamp_millis(),
                };

                if let Err(e) = self.cache.set(&sort_cache_key, sort_cache.clone(), FRIEND_LIST_CACHE_TTL_SECS).await {
                    ::tracing::warn!(
                        user_id = %user_id,
                        cache_key = %sort_cache_key,
                        error = %e,
                        "Failed to cache friend list sort result"
                    );
                }

                sort_cache
            }
        };

        // 借引用避免 Vec<FriendListEntry> 整体 move；paged_items 通过
        // .cloned() 显式 clone 命中的 50 个 entry，零额外 Vec 移动。
        let items: &[FriendListEntry] = &sort_cache.items;
        let total = sort_cache.total;
        // W6: cursor 翻页起点解析。partition_point 二分 O(log n)，
        // 消除 W3 review TODO 4 的 O(n) scan。
        let start_index = resolve_cursor_start_index(items, &request);
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

        // 缓存语义：v4 路径下 `cached: true` 表示「排序结果复用」（即跳过了 DB 查询
        // 和排序计算），分页是 O(1) in-memory operation，本身不缓存。
        // 这与 v3 的 "整个 page 缓存" 语义不同，但 v3 在 limit 变体下命中率 ~0%，
        // v4 在 sort_by 不变时命中率 100%。
        let page = FriendListPage {
            room_id,
            items: paged_items,
            total,
            limit: safe_limit,
            offset: request.from.is_none().then_some(start_index),
            next_offset,
            next_batch,
            version,
            cached: sort_cache_hit,
            generated_ts: current_timestamp_millis(),
        };

        // 抑制 unused warning
        Ok(page)
    }

    /// See [`sync_dm_room_membership_change`].
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
            .map_err(|e| ApiError::database_with_context("Failed to load friend DM links", &e))?;

        if links.is_empty() {
            return Ok(0);
        }

        let now = current_timestamp_millis();

        // B-1.4: Phase 1 — single SQL batch reads, then sequential in-memory
        // shard processing so we hold only `&self` at any point (no concurrent
        // mutable borrow of `self`).
        //
        // Before: per-link `get_friend_list_all_shards` was called inside the
        // fan-out for loop — 5 links × 28 shards fan-out → 5 sequential DB
        // roundtrips.
        //
        // After: one `get_friend_list_all_shards_batch` SQL returns all
        // `(friend_room_id, state_key, content)` rows; in-memory aggregation
        // by `friend_room_id` keeps the existing for-link body unchanged.
        //
        // Phase 2 — fire ALL state-event writes concurrently. Each
        // `send_state_event_inner` clones the room_service Arc so the borrow
        // checker is satisfied. Before: each `link × shard` pair was written
        // serially (O(N×M) async steps). After: a single `try_join_all` fans
        // out all writes (total wall time ≈ max latency of the slowest write).
        let mut all_writes: Vec<(String, String, String, Value)> = Vec::new();

        let shard_index: std::collections::HashMap<String, Vec<(String, Value)>> = {
            let room_ids: Vec<String> = links.iter().map(|link| link.friend_room_id.clone()).collect();
            // Empty-link fast path: skip SQL entirely.
            if room_ids.is_empty() {
                std::collections::HashMap::new()
            } else {
                self.friend_storage
                    .get_friend_list_all_shards_batch(&room_ids)
                    .await
                    .map_err(|e| ApiError::database_with_context("Failed to fan-out friend list shards (batch)", &e))?
            }
        };

        for link in links {
            // W5 sharding：find_friend_lists_by_dm_room_id 内部 SQL 写死 state_key=''，
            // 老 v4 时代会直接返回 friend list content；W5 后 owner 的 friend 散在 28 个
            // shard 里，因此 service 端必须重新 fan-out 读 all_shards 拿全量。
            // link.content 在 W5 体系下语义不完整（只反映 legacy 通道），直接丢弃。
            //
            // B-1.4: shard_index pre-loaded via batch SQL — no per-link DB read.
            let Some(shards) = shard_index.get(&link.friend_room_id) else {
                // No shards for this room_id (shouldn't normally happen since
                // find_friend_lists_by_dm_room_id returned a link for it,
                // but stay defensive: skip).
                continue;
            };

            // 找出 dm_room_id 命中的 friend 所在 shard。
            // 同一个 dm_room_id 可能在不同 shard 各被一个 friend 引用（不常见但可能），
            // 因此需逐 shard 检查。
            for (state_key, shard_content) in shards {
                let mut shard_content = shard_content.clone();
                let mut touched = false;
                if let Some(friends) = shard_content.get_mut("friends").and_then(|value| value.as_array_mut()) {
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
                if touched {
                    if let Some(version) = shard_content.get("version").and_then(|value| value.as_i64()) {
                        shard_content["version"] = json!(version + 1);
                    }
                    all_writes.push((
                        link.friend_room_id.clone(),
                        link.owner_user_id.clone(),
                        state_key.clone(),
                        shard_content,
                    ));
                }
            }
        }

        // Phase 2: concurrent state writes
        let room_service = Arc::clone(&self.room_service);
        let server_name = self.server_name.clone();

        let write_futures: Vec<_> = all_writes
            .into_iter()
            .map(|(room_id, user_id, state_key, content)| {
                let room_service = Arc::clone(&room_service);
                let server_name = &server_name;
                #[allow(clippy::needless_borrow)]
                async move {
                    Self::send_state_event_inner(
                        &*room_service,
                        &server_name,
                        &room_id,
                        &user_id,
                        "m.friends.list",
                        &state_key,
                        content,
                    )
                    .await
                }
            })
            .collect();

        let write_count = write_futures.len();

        try_join_all(write_futures).await?;

        Ok(write_count)
    }

    /// Stateless helper that sends a state event, accepting an explicit `Arc` so
    /// callers can clone the reference for concurrent execution.
    #[allow(clippy::needless_borrow)]
    async fn send_state_event_inner(
        room_service: &(dyn crate::room::RoomServiceApi + '_),
        server_name: &str,
        room_id: &str,
        user_id: &str,
        event_type: &str,
        state_key: &str,
        content: Value,
    ) -> ApiResult<()> {
        let now = current_timestamp_millis();
        room_service
            .messaging()
            .create_event(
                CreateEventParams {
                    event_id: generate_event_id(server_name),
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
            .map(|_room_event| ())
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
            })
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
                    ApiError::database_with_context("Failed to create friend request: user not found", &error_msg)
                } else {
                    ApiError::database_with_context("Failed to create friend request", &error_msg)
                }
            },
        )?;

        Ok(())
    }

    // --- Helpers ---

    /// See [`is_remote_user`].
    pub(crate) fn is_remote_user(&self, user_id: &str) -> bool {
        !user_id.ends_with(&format!(":{}", self.server_name))
    }

    /// See [`send_state_event`].
    pub(crate) async fn send_state_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
        state_key: &str,
        content: serde_json::Value,
    ) -> ApiResult<()> {
        Self::send_state_event_inner(
            &*self.room_service,
            &self.server_name,
            room_id,
            user_id,
            event_type,
            state_key,
            content,
        )
        .await
    }

    async fn update_friend_list(
        &self,
        user_id: &str,
        room_id: &str,
        friend_id: &str,
        action: &str,
        dm_room_id: Option<&str>,
    ) -> ApiResult<()> {
        // W5 sharding：按 friend_id 路由到对应 shard，只改该 shard。
        // 同 shard 内 add/remove 不动其他 shard，避免单 event 超过 2704 字节上限。
        // v4 遗留好友可能在 legacy state_key=""，read_friend_shard_for_update 会回退定位。
        let (state_key, mut content) = read_friend_shard_for_update(self.friend_storage.as_ref(), room_id, friend_id)
            .await
            .map_err(|e| ApiError::database_with_context("Database error", &e))?;

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

        self.send_state_event(room_id, user_id, "m.friends.list", &state_key, content).await?;
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
            .map_err(|e| ApiError::database_with_context("Database error", &e))?
        else {
            return Ok(false);
        };

        // v4 遗留好友可能在 legacy state_key=""，read_friend_shard_for_update 会回退定位。
        let (state_key, mut content) =
            read_friend_shard_for_update(self.friend_storage.as_ref(), &friend_room_id, friend_id)
                .await
                .map_err(|e| ApiError::database_with_context("Database error", &e))?;

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

        self.send_state_event(&friend_room_id, owner_user_id, "m.friends.list", &state_key, content).await?;

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

    /// See [`build_friend_entries`].
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

    /// See [`sort_friend_entries`].
    pub(crate) fn sort_friend_entries(items: &mut [FriendListEntry], sort_by: &str) {
        items.sort_by(|left, right| Self::compare_friend_entries(left, right, sort_by));
    }

    /// See [`compare_friend_entries`].
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

    /// See [`compare_friend_entry_to_cursor`].
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

    /// See [`cursor_from_friend_entry`].
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

    /// See [`friend_display_key`].
    pub(crate) fn friend_display_key(item: &FriendListEntry) -> &str {
        item.display_name.as_deref().or(item.username.as_deref()).unwrap_or(item.user_id.as_str())
    }

    /// See [`build_direct_room_snapshot`].
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
mod tests;

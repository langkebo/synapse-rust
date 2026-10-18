//! B-3：把好友「拉黑」接进入准入判断。
//!
//! `blocked` 一直只是 `m.friends.list` 里的一个状态标签，没有任何调用方读它做判断，
//! 于是「拉黑」形同虚设。这里把它落成一个可复用的判定器：
//!
//! - 邀请门禁（`InviteBlocklistService::check_invite_allowed`）通过
//!   [`BlockedRelationChecker`] 拿到它；
//! - 好友请求 / 建 DM 在 `FriendRoomService` 内部直接调用
//!   [`FriendRoomService::is_blocked_by`]。
//!
//! 方向是**单向**的：`has_blocked(owner, other)` 只回答「owner 是否拉黑了 other」。
//! 是否拒绝某个动作，由调用方按「被作用方是否拉黑了发起方」来决定。

use std::sync::Arc;
use synapse_common::error::{ApiError, ApiResult};
use synapse_storage::friend_room::FriendRoomStorage;

use crate::invite_blocklist_service::BlockedRelationChecker;

/// 基于好友状态表（`m.friends.list`）的拉黑判定。
pub struct FriendStatusBlockedRelationChecker {
    storage: Arc<FriendRoomStorage>,
}

impl FriendStatusBlockedRelationChecker {
    /// See [`new`].
    pub fn new(storage: Arc<FriendRoomStorage>) -> Self {
        Self { storage }
    }
}

#[async_trait::async_trait]
impl BlockedRelationChecker for FriendStatusBlockedRelationChecker {
    async fn has_blocked(&self, owner_id: &str, other_id: &str) -> ApiResult<bool> {
        is_blocked_by(self.storage.as_ref(), owner_id, other_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to read friend status", e))
    }
}

/// 读「`owner_id` 是否拉黑了 `other_id`」。
///
/// 刻意**不**走 `FriendRoomService::create_friend_list_room`：
/// 那会在「拉黑方还没有好友房」时**顺手建一个房间**。准入判断是只读路径，
/// 为了查一个布尔值就去写一条房间 + 状态事件，代价和副作用都不该有。
/// 这里先查好友房 id，查不到就等价于「没有拉黑任何人」。
pub(crate) async fn is_blocked_by(
    storage: &FriendRoomStorage,
    owner_id: &str,
    other_id: &str,
) -> Result<bool, sqlx::Error> {
    if owner_id == other_id {
        return Ok(false);
    }

    let Some(friend_room_id) = storage.get_friend_list_room_id(owner_id).await? else {
        return Ok(false);
    };
    let Some(info) = storage.get_friend_info(&friend_room_id, other_id).await? else {
        return Ok(false);
    };

    Ok(info.get("status").and_then(|v| v.as_str()) == Some("blocked"))
}

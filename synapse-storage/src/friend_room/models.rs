/// The `FriendDmLink` struct.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FriendDmLink {
    /// The `owner_user_id` field.
    pub owner_user_id: String,
    /// The `friend_room_id` field.
    pub friend_room_id: String,
    /// The `content` field.
    pub content: serde_json::Value,
}

/// The `DirectRoomFallbackLink` struct.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DirectRoomFallbackLink {
    /// The `other_user_id` field.
    pub other_user_id: String,
    /// The `room_id` field.
    pub room_id: String,
}

/// The `DmPartnerRecord` struct.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DmPartnerRecord {
    /// The `user_id` field.
    pub user_id: String,
    /// The `display_name` field.
    pub display_name: String,
    /// The `avatar_url` field.
    pub avatar_url: String,
}

/// 创建好友分组的参数
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateFriendGroupParams {
    /// The `room_id` field.
    pub room_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `group_name` field.
    pub group_name: String,
}

/// 添加好友到分组的参数
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AddFriendToGroupParams {
    /// The `room_id` field.
    pub room_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `group_name` field.
    pub group_name: String,
    /// The `friend_id` field.
    pub friend_id: String,
}

/// 从分组移除好友的参数
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RemoveFriendFromGroupParams {
    /// The `room_id` field.
    pub room_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `group_name` field.
    pub group_name: String,
    /// The `friend_id` field.
    pub friend_id: String,
}

/// 重命名好友分组的参数
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RenameFriendGroupParams {
    /// The `room_id` field.
    pub room_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `old_group_name` field.
    pub old_group_name: String,
    /// The `new_group_name` field.
    pub new_group_name: String,
}

/// The `FriendRequestRecord` struct.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct FriendRequestRecord {
    /// The `id` field.
    pub id: i64,
    /// The `sender_id` field.
    pub sender_id: String,
    /// The `receiver_id` field.
    pub receiver_id: String,
    /// The `message` field.
    pub message: Option<String>,
    /// The `status` field.
    pub status: String,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FriendDmLink {
    pub owner_user_id: String,
    pub friend_room_id: String,
    pub content: serde_json::Value,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DirectRoomFallbackLink {
    pub other_user_id: String,
    pub room_id: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DmPartnerRecord {
    pub user_id: String,
    pub display_name: String,
    pub avatar_url: String,
}

/// 创建好友分组的参数
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateFriendGroupParams {
    pub room_id: String,
    pub user_id: String,
    pub group_name: String,
}

/// 添加好友到分组的参数
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AddFriendToGroupParams {
    pub room_id: String,
    pub user_id: String,
    pub group_name: String,
    pub friend_id: String,
}

/// 从分组移除好友的参数
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RemoveFriendFromGroupParams {
    pub room_id: String,
    pub user_id: String,
    pub group_name: String,
    pub friend_id: String,
}

/// 重命名好友分组的参数
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RenameFriendGroupParams {
    pub room_id: String,
    pub user_id: String,
    pub old_group_name: String,
    pub new_group_name: String,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct FriendRequestRecord {
    pub id: i64,
    pub sender_id: String,
    pub receiver_id: String,
    pub message: Option<String>,
    pub status: String,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

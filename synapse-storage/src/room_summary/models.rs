use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RoomSummary {
    pub id: Option<i64>,
    pub room_id: String,
    pub room_type: Option<String>,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub canonical_alias: Option<String>,
    #[sqlx(rename = "join_rules")]
    pub join_rule: String,
    pub history_visibility: String,
    pub guest_access: String,
    pub is_direct: bool,
    pub is_space: bool,
    pub is_encrypted: bool,
    pub member_count: Option<i64>,
    pub joined_member_count: Option<i64>,
    pub invited_member_count: Option<i64>,
    pub hero_users: serde_json::Value,
    pub last_event_id: Option<String>,
    pub last_event_ts: Option<i64>,
    pub last_message_ts: Option<i64>,
    pub unread_notifications: i64,
    pub unread_highlight: i64,
    pub updated_ts: Option<i64>,
    pub created_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RoomSummaryMember {
    pub id: i64,
    pub room_id: String,
    pub user_id: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub membership: String,
    pub is_hero: bool,
    pub last_active_ts: Option<i64>,
    pub updated_ts: i64,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RoomSummaryState {
    pub id: i64,
    pub room_id: String,
    pub event_type: String,
    pub state_key: String,
    pub event_id: Option<String>,
    pub content: serde_json::Value,
    pub updated_ts: i64,
}

/// Input entry for batch upserts via [`RoomSummaryStorage::set_states_batch`].
#[derive(Debug, Clone)]
pub struct RoomSummaryStateEntry {
    pub event_type: String,
    pub state_key: String,
    pub event_id: Option<String>,
    pub content: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RoomSummaryStats {
    pub id: i64,
    pub room_id: String,
    pub total_events: i64,
    pub total_state_events: i64,
    pub total_messages: i64,
    pub total_media: i64,
    pub storage_size: i64,
    pub last_updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoomSummaryRequest {
    pub room_id: String,
    pub room_type: Option<String>,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub canonical_alias: Option<String>,
    pub join_rule: Option<String>,
    pub history_visibility: Option<String>,
    pub guest_access: Option<String>,
    pub is_direct: Option<bool>,
    pub is_space: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateRoomSummaryRequest {
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub canonical_alias: Option<String>,
    pub join_rule: Option<String>,
    pub history_visibility: Option<String>,
    pub guest_access: Option<String>,
    pub is_direct: Option<bool>,
    pub is_space: Option<bool>,
    pub is_encrypted: Option<bool>,
    pub last_event_id: Option<String>,
    pub last_event_ts: Option<i64>,
    pub last_message_ts: Option<i64>,
    pub hero_users: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSummaryMemberRequest {
    pub room_id: String,
    pub user_id: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub membership: String,
    pub is_hero: Option<bool>,
    pub last_active_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSummaryMemberRequest {
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub membership: Option<String>,
    pub is_hero: Option<bool>,
    pub last_active_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomSummaryResponse {
    pub room_id: String,
    pub room_type: Option<String>,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub canonical_alias: Option<String>,
    pub join_rule: String,
    pub history_visibility: String,
    pub guest_access: String,
    pub is_direct: bool,
    pub is_space: bool,
    pub is_encrypted: bool,
    pub member_count: i64,
    pub joined_member_count: i64,
    pub invited_member_count: i64,
    pub heroes: Vec<RoomSummaryHero>,
    pub last_event_ts: Option<i64>,
    pub last_message_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomSummaryHero {
    pub user_id: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct RoomSummaryUpdateQueueItem {
    pub id: i64,
    pub room_id: String,
    pub event_id: String,
    pub event_type: String,
    pub state_key: Option<String>,
    pub priority: i32,
    pub status: String,
    pub created_ts: i64,
    pub processed_ts: Option<i64>,
    pub error_message: Option<String>,
    pub retry_count: i32,
}

impl RoomSummary {
    pub fn to_response(&self, heroes: Vec<RoomSummaryHero>) -> RoomSummaryResponse {
        RoomSummaryResponse {
            room_id: self.room_id.clone(),
            room_type: self.room_type.clone(),
            name: self.name.clone(),
            topic: self.topic.clone(),
            avatar_url: self.avatar_url.clone(),
            canonical_alias: self.canonical_alias.clone(),
            join_rule: self.join_rule.clone(),
            history_visibility: self.history_visibility.clone(),
            guest_access: self.guest_access.clone(),
            is_direct: self.is_direct,
            is_space: self.is_space,
            is_encrypted: self.is_encrypted,
            member_count: self.member_count.unwrap_or(0),
            joined_member_count: self.joined_member_count.unwrap_or(0),
            invited_member_count: self.invited_member_count.unwrap_or(0),
            heroes,
            last_event_ts: self.last_event_ts,
            last_message_ts: self.last_message_ts,
        }
    }
}

impl From<RoomSummaryMember> for RoomSummaryHero {
    fn from(member: RoomSummaryMember) -> Self {
        Self { user_id: member.user_id, display_name: member.display_name, avatar_url: member.avatar_url }
    }
}

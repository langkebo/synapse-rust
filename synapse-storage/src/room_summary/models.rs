use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// The `RoomSummary` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RoomSummary {
    /// The `id` field.
    pub id: Option<i64>,
    /// The `room_id` field.
    pub room_id: String,
    /// The `room_type` field.
    pub room_type: Option<String>,
    /// The `name` field.
    pub name: Option<String>,
    /// The `topic` field.
    pub topic: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `canonical_alias` field.
    pub canonical_alias: Option<String>,
    #[sqlx(rename = "join_rules")]
    /// The `join_rule` field.
    pub join_rule: String,
    /// The `history_visibility` field.
    pub history_visibility: String,
    /// The `guest_access` field.
    pub guest_access: String,
    /// The `is_direct` field.
    pub is_direct: bool,
    /// The `is_space` field.
    pub is_space: bool,
    /// The `is_encrypted` field.
    pub is_encrypted: bool,
    /// The `member_count` field.
    pub member_count: Option<i64>,
    /// The `joined_member_count` field.
    pub joined_member_count: Option<i64>,
    /// The `invited_member_count` field.
    pub invited_member_count: Option<i64>,
    /// The `hero_users` field.
    pub hero_users: serde_json::Value,
    /// The `last_event_id` field.
    pub last_event_id: Option<String>,
    /// The `last_event_ts` field.
    pub last_event_ts: Option<i64>,
    /// The `last_message_ts` field.
    pub last_message_ts: Option<i64>,
    /// The `unread_notifications` field.
    pub unread_notifications: i64,
    /// The `unread_highlight` field.
    pub unread_highlight: i64,
    /// The `updated_ts` field.
    pub updated_ts: Option<i64>,
    /// The `created_ts` field.
    pub created_ts: Option<i64>,
}

/// The `RoomSummaryMember` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RoomSummaryMember {
    /// The `id` field.
    pub id: i64,
    /// The `room_id` field.
    pub room_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `display_name` field.
    pub display_name: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `membership` field.
    pub membership: String,
    /// The `is_hero` field.
    pub is_hero: bool,
    /// The `last_active_ts` field.
    pub last_active_ts: Option<i64>,
    /// The `updated_ts` field.
    pub updated_ts: i64,
    /// The `created_ts` field.
    pub created_ts: i64,
}

/// The `RoomSummaryState` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RoomSummaryState {
    /// The `id` field.
    pub id: i64,
    /// The `room_id` field.
    pub room_id: String,
    /// The `event_type` field.
    pub event_type: String,
    /// The `state_key` field.
    pub state_key: String,
    /// The `event_id` field.
    pub event_id: Option<String>,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// Input entry for batch upserts via [`RoomSummaryStorage::set_states_batch`].
#[derive(Debug, Clone)]
pub struct RoomSummaryStateEntry {
    /// The `event_type` field.
    pub event_type: String,
    /// The `state_key` field.
    pub state_key: String,
    /// The `event_id` field.
    pub event_id: Option<String>,
    /// The `content` field.
    pub content: serde_json::Value,
}

/// The `RoomSummaryStats` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RoomSummaryStats {
    /// The `id` field.
    pub id: i64,
    /// The `room_id` field.
    pub room_id: String,
    /// The `total_events` field.
    pub total_events: i64,
    /// The `total_state_events` field.
    pub total_state_events: i64,
    /// The `total_messages` field.
    pub total_messages: i64,
    /// The `total_media` field.
    pub total_media: i64,
    /// The `storage_size` field.
    pub storage_size: i64,
    /// The `last_updated_ts` field.
    pub last_updated_ts: i64,
}

/// The `CreateRoomSummaryRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoomSummaryRequest {
    /// The `room_id` field.
    pub room_id: String,
    /// The `room_type` field.
    pub room_type: Option<String>,
    /// The `name` field.
    pub name: Option<String>,
    /// The `topic` field.
    pub topic: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `canonical_alias` field.
    pub canonical_alias: Option<String>,
    /// The `join_rule` field.
    pub join_rule: Option<String>,
    /// The `history_visibility` field.
    pub history_visibility: Option<String>,
    /// The `guest_access` field.
    pub guest_access: Option<String>,
    /// The `is_direct` field.
    pub is_direct: Option<bool>,
    /// The `is_space` field.
    pub is_space: Option<bool>,
}

/// The `UpdateRoomSummaryRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateRoomSummaryRequest {
    /// The `name` field.
    pub name: Option<String>,
    /// The `topic` field.
    pub topic: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `canonical_alias` field.
    pub canonical_alias: Option<String>,
    /// The `join_rule` field.
    pub join_rule: Option<String>,
    /// The `history_visibility` field.
    pub history_visibility: Option<String>,
    /// The `guest_access` field.
    pub guest_access: Option<String>,
    /// The `is_direct` field.
    pub is_direct: Option<bool>,
    /// The `is_space` field.
    pub is_space: Option<bool>,
    /// The `is_encrypted` field.
    pub is_encrypted: Option<bool>,
    /// The `last_event_id` field.
    pub last_event_id: Option<String>,
    /// The `last_event_ts` field.
    pub last_event_ts: Option<i64>,
    /// The `last_message_ts` field.
    pub last_message_ts: Option<i64>,
    /// The `hero_users` field.
    pub hero_users: Option<serde_json::Value>,
}

/// The `CreateSummaryMemberRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSummaryMemberRequest {
    /// The `room_id` field.
    pub room_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `display_name` field.
    pub display_name: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `membership` field.
    pub membership: String,
    /// The `is_hero` field.
    pub is_hero: Option<bool>,
    /// The `last_active_ts` field.
    pub last_active_ts: Option<i64>,
}

/// The `UpdateSummaryMemberRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSummaryMemberRequest {
    /// The `display_name` field.
    pub display_name: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `membership` field.
    pub membership: Option<String>,
    /// The `is_hero` field.
    pub is_hero: Option<bool>,
    /// The `last_active_ts` field.
    pub last_active_ts: Option<i64>,
}

/// The `RoomSummaryResponse` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomSummaryResponse {
    /// The `room_id` field.
    pub room_id: String,
    /// The `room_type` field.
    pub room_type: Option<String>,
    /// The `name` field.
    pub name: Option<String>,
    /// The `topic` field.
    pub topic: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `canonical_alias` field.
    pub canonical_alias: Option<String>,
    /// The `join_rule` field.
    pub join_rule: String,
    /// The `history_visibility` field.
    pub history_visibility: String,
    /// The `guest_access` field.
    pub guest_access: String,
    /// The `is_direct` field.
    pub is_direct: bool,
    /// The `is_space` field.
    pub is_space: bool,
    /// The `is_encrypted` field.
    pub is_encrypted: bool,
    /// The `member_count` field.
    pub member_count: i64,
    /// The `joined_member_count` field.
    pub joined_member_count: i64,
    /// The `invited_member_count` field.
    pub invited_member_count: i64,
    /// The `heroes` field.
    pub heroes: Vec<RoomSummaryHero>,
    /// The `last_event_ts` field.
    pub last_event_ts: Option<i64>,
    /// The `last_message_ts` field.
    pub last_message_ts: Option<i64>,
    /// `allowed_room_ids` from `m.room.join_rules` when the join rule is
    /// `restricted` or `knock_restricted` (Matrix v1.15). `None` for any
    /// other join rule; `Some(vec)` for restricted rules (may be empty).
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The `allowed_room_ids` field.
    pub allowed_room_ids: Option<Vec<String>>,
}

/// The `RoomSummaryHero` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomSummaryHero {
    /// The `user_id` field.
    pub user_id: String,
    /// The `display_name` field.
    pub display_name: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
}

/// The `RoomSummaryUpdateQueueItem` struct.
#[derive(Debug, Clone, FromRow)]
pub struct RoomSummaryUpdateQueueItem {
    /// The `id` field.
    pub id: i64,
    /// The `room_id` field.
    pub room_id: String,
    /// The `event_id` field.
    pub event_id: String,
    /// The `event_type` field.
    pub event_type: String,
    /// The `state_key` field.
    pub state_key: Option<String>,
    /// The `priority` field.
    pub priority: i32,
    /// The `status` field.
    pub status: String,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `processed_ts` field.
    pub processed_ts: Option<i64>,
    /// The `error_message` field.
    pub error_message: Option<String>,
    /// The `retry_count` field.
    pub retry_count: i32,
}

impl RoomSummary {
    /// See [`to_response`].
    /// See [`to_response`].
    pub fn to_response(&self, heroes: Vec<RoomSummaryHero>) -> RoomSummaryResponse {
        self.to_response_with_allowed_room_ids(heroes, None)
    }

    /// Build a [`RoomSummaryResponse`] with an explicit `allowed_room_ids`
    /// value, extracted from the room's `m.room.join_rules` state event by
    /// the caller. Use [`to_response`] when `allowed_room_ids` is not
    /// available (defaults to `None`).
    pub fn to_response_with_allowed_room_ids(
        &self,
        heroes: Vec<RoomSummaryHero>,
        allowed_room_ids: Option<Vec<String>>,
    ) -> RoomSummaryResponse {
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
            allowed_room_ids,
        }
    }
}

impl From<RoomSummaryMember> for RoomSummaryHero {
    fn from(member: RoomSummaryMember) -> Self {
        Self { user_id: member.user_id, display_name: member.display_name, avatar_url: member.avatar_url }
    }
}

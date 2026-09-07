use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// The `Space` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Space {
    /// The `space_id` field.
    pub space_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `name` field.
    pub name: Option<String>,
    /// The `topic` field.
    pub topic: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `creator` field.
    pub creator: String,
    /// The `join_rule` field.
    pub join_rule: String,
    /// The `visibility` field.
    pub visibility: Option<String>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: Option<i64>,
    /// The `is_public` field.
    pub is_public: bool,
    /// The `parent_space_id` field.
    pub parent_space_id: Option<String>,
    /// The `room_type` field.
    pub room_type: Option<String>,
}

/// The `SpaceChild` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SpaceChild {
    /// The `id` field.
    pub id: i64,
    /// The `space_id` field.
    pub space_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `sender` field.
    pub sender: String,
    /// The `is_suggested` field.
    pub is_suggested: bool,
    /// The `via_servers` field.
    pub via_servers: Vec<String>,
    /// The `added_ts` field.
    pub added_ts: i64,
    /// The `order` field.
    pub order: Option<String>,
    /// The `suggested` field.
    pub suggested: Option<bool>,
    /// The `added_by` field.
    pub added_by: Option<String>,
    /// The `removed_ts` field.
    pub removed_ts: Option<i64>,
}

/// The `SpaceMember` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SpaceMember {
    /// The `space_id` field.
    pub space_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `membership` field.
    pub membership: String,
    /// The `joined_ts` field.
    pub joined_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: Option<i64>,
    /// The `left_ts` field.
    pub left_ts: Option<i64>,
    /// The `inviter` field.
    pub inviter: Option<String>,
}

/// The `SpaceSummary` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SpaceSummary {
    /// The `id` field.
    pub id: i64,
    /// The `space_id` field.
    pub space_id: String,
    /// The `summary` field.
    pub summary: serde_json::Value,
    /// The `children_count` field.
    pub children_count: Option<i64>,
    /// The `member_count` field.
    pub member_count: Option<i64>,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// The `SpaceEvent` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SpaceEvent {
    /// The `event_id` field.
    pub event_id: String,
    /// The `space_id` field.
    pub space_id: String,
    /// The `event_type` field.
    pub event_type: String,
    /// The `sender` field.
    pub sender: String,
    /// The `content` field.
    pub content: serde_json::Value,
    /// The `state_key` field.
    pub state_key: Option<String>,
    /// The `origin_server_ts` field.
    pub origin_server_ts: i64,
    /// The `processed_ts` field.
    pub processed_ts: Option<i64>,
}

/// The `CreateSpaceRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSpaceRequest {
    /// The `room_id` field.
    pub room_id: String,
    /// The `name` field.
    pub name: Option<String>,
    /// The `topic` field.
    pub topic: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `creator` field.
    pub creator: String,
    /// The `join_rule` field.
    pub join_rule: Option<String>,
    /// The `visibility` field.
    pub visibility: Option<String>,
    /// The `is_public` field.
    pub is_public: Option<bool>,
    /// The `parent_space_id` field.
    pub parent_space_id: Option<String>,
}

/// The `AddChildRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddChildRequest {
    /// The `space_id` field.
    pub space_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `sender` field.
    pub sender: String,
    /// The `is_suggested` field.
    pub is_suggested: bool,
    /// The `via_servers` field.
    pub via_servers: Vec<String>,
}

/// The `UpdateSpaceRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateSpaceRequest {
    /// The `name` field.
    pub name: Option<String>,
    /// The `topic` field.
    pub topic: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `join_rule` field.
    pub join_rule: Option<String>,
    /// The `visibility` field.
    pub visibility: Option<String>,
    /// The `is_public` field.
    pub is_public: Option<bool>,
}

impl UpdateSpaceRequest {
    /// See [`new`].
    /// See [`new`].
    pub fn new() -> Self {
        Self::default()
    }

    /// See [`name`].
    /// See [`name`].
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// See [`topic`].
    /// See [`topic`].
    pub fn topic(mut self, topic: impl Into<String>) -> Self {
        self.topic = Some(topic.into());
        self
    }

    /// See [`avatar_url`].
    /// See [`avatar_url`].
    pub fn avatar_url(mut self, avatar_url: impl Into<String>) -> Self {
        self.avatar_url = Some(avatar_url.into());
        self
    }

    /// See [`join_rule`].
    /// See [`join_rule`].
    pub fn join_rule(mut self, join_rule: impl Into<String>) -> Self {
        self.join_rule = Some(join_rule.into());
        self
    }

    /// See [`visibility`].
    /// See [`visibility`].
    pub fn visibility(mut self, visibility: impl Into<String>) -> Self {
        self.visibility = Some(visibility.into());
        self
    }

    /// See [`is_public`].
    /// See [`is_public`].
    pub fn is_public(mut self, is_public: bool) -> Self {
        self.is_public = Some(is_public);
        self
    }
}

/// The `SpaceHierarchy` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceHierarchy {
    /// The `space` field.
    pub space: Space,
    /// The `children` field.
    pub children: Vec<SpaceChild>,
    /// The `members` field.
    pub members: Vec<SpaceMember>,
}

/// The `SpaceHierarchyNode` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceHierarchyNode {
    /// The `space` field.
    pub space: Space,
    /// The `children` field.
    pub children: Vec<Self>,
    /// The `depth` field.
    pub depth: i32,
}

/// The `SpaceHierarchyRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceHierarchyRequest {
    /// The `space_id` field.
    pub space_id: String,
    /// The `max_depth` field.
    pub max_depth: i32,
    /// The `suggested_only` field.
    pub suggested_only: bool,
    /// The `limit` field.
    pub limit: Option<i32>,
    /// The `from` field.
    pub from: Option<String>,
}

/// The `SpaceHierarchyResponse` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceHierarchyResponse {
    /// The `rooms` field.
    pub rooms: Vec<SpaceHierarchyRoom>,
    /// The `next_batch` field.
    pub next_batch: Option<String>,
}

/// The `SpaceHierarchyRoom` struct.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SpaceHierarchyRoom {
    /// The `room_id` field.
    pub room_id: String,
    /// The `name` field.
    pub name: Option<String>,
    /// The `topic` field.
    pub topic: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `join_rule` field.
    pub join_rule: String,
    /// The `world_readable` field.
    pub world_readable: bool,
    /// The `guest_can_join` field.
    pub guest_can_join: bool,
    /// The `num_joined_members` field.
    pub num_joined_members: i64,
    /// The `room_type` field.
    pub room_type: Option<String>,
    /// The `children_state` field.
    pub children_state: Vec<serde_json::Value>,
}

/// The `SpaceChildInfo` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceChildInfo {
    /// The `space_id` field.
    pub space_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `via_servers` field.
    pub via_servers: Vec<String>,
    /// The `is_suggested` field.
    pub is_suggested: bool,
    /// The `is_space` field.
    pub is_space: bool,
    /// The `depth` field.
    pub depth: i32,
}

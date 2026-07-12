use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Space {
    pub space_id: String,
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub creator: String,
    pub join_rule: String,
    pub visibility: Option<String>,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub is_public: bool,
    pub parent_space_id: Option<String>,
    pub room_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SpaceChild {
    pub id: i64,
    pub space_id: String,
    pub room_id: String,
    pub sender: String,
    pub is_suggested: bool,
    pub via_servers: Vec<String>,
    pub added_ts: i64,
    pub order: Option<String>,
    pub suggested: Option<bool>,
    pub added_by: Option<String>,
    pub removed_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SpaceMember {
    pub space_id: String,
    pub user_id: String,
    pub membership: String,
    pub joined_ts: i64,
    pub updated_ts: Option<i64>,
    pub left_ts: Option<i64>,
    pub inviter: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SpaceSummary {
    pub id: i64,
    pub space_id: String,
    pub summary: serde_json::Value,
    pub children_count: Option<i64>,
    pub member_count: Option<i64>,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SpaceEvent {
    pub event_id: String,
    pub space_id: String,
    pub event_type: String,
    pub sender: String,
    pub content: serde_json::Value,
    pub state_key: Option<String>,
    pub origin_server_ts: i64,
    pub processed_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSpaceRequest {
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub creator: String,
    pub join_rule: Option<String>,
    pub visibility: Option<String>,
    pub is_public: Option<bool>,
    pub parent_space_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddChildRequest {
    pub space_id: String,
    pub room_id: String,
    pub sender: String,
    pub is_suggested: bool,
    pub via_servers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateSpaceRequest {
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub join_rule: Option<String>,
    pub visibility: Option<String>,
    pub is_public: Option<bool>,
}

impl UpdateSpaceRequest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn topic(mut self, topic: impl Into<String>) -> Self {
        self.topic = Some(topic.into());
        self
    }

    pub fn avatar_url(mut self, avatar_url: impl Into<String>) -> Self {
        self.avatar_url = Some(avatar_url.into());
        self
    }

    pub fn join_rule(mut self, join_rule: impl Into<String>) -> Self {
        self.join_rule = Some(join_rule.into());
        self
    }

    pub fn visibility(mut self, visibility: impl Into<String>) -> Self {
        self.visibility = Some(visibility.into());
        self
    }

    pub fn is_public(mut self, is_public: bool) -> Self {
        self.is_public = Some(is_public);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceHierarchy {
    pub space: Space,
    pub children: Vec<SpaceChild>,
    pub members: Vec<SpaceMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceHierarchyNode {
    pub space: Space,
    pub children: Vec<Self>,
    pub depth: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceHierarchyRequest {
    pub space_id: String,
    pub max_depth: i32,
    pub suggested_only: bool,
    pub limit: Option<i32>,
    pub from: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceHierarchyResponse {
    pub rooms: Vec<SpaceHierarchyRoom>,
    pub next_batch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SpaceHierarchyRoom {
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub join_rule: String,
    pub world_readable: bool,
    pub guest_can_join: bool,
    pub num_joined_members: i64,
    pub room_type: Option<String>,
    pub children_state: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceChildInfo {
    pub space_id: String,
    pub room_id: String,
    pub via_servers: Vec<String>,
    pub is_suggested: bool,
    pub is_space: bool,
    pub depth: i32,
}

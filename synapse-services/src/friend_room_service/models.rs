use crate::RoomService;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_federation::friend::FriendFederationClient;
use synapse_storage::{EventStorage, FriendRoomStorage, PresenceStorage, UserStorage};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendListRequest {
    pub limit: usize,
    pub offset: usize,
    pub sort_by: String,
}

impl Default for FriendListRequest {
    fn default() -> Self {
        Self { limit: 50, offset: 0, sort_by: "alphabet".to_string() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendListEntry {
    pub user_id: String,
    pub username: Option<String>,
    #[serde(rename = "displayname")]
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub note: Option<String>,
    pub status: String,
    pub online: bool,
    pub presence: String,
    pub last_active_ts: Option<i64>,
    pub last_seen_ts: Option<i64>,
    pub added_ts: Option<i64>,
    pub sort_letter: String,
    pub dm_room_id: Option<String>,
    pub dm_room_active: bool,
    pub dm_room_state: Option<String>,
    pub dm_room_updated_ts: Option<i64>,
    pub dm_room_affected_user_id: Option<String>,
    pub dm_room_changed_by: Option<String>,
    pub dm_room_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendListPage {
    pub room_id: String,
    pub items: Vec<FriendListEntry>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    pub next_offset: Option<usize>,
    pub version: i64,
    pub cached: bool,
    pub generated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmPartnerInfo {
    pub user_id: String,
    pub display_name: String,
    pub avatar_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnsureDirectRoomResult {
    pub room_id: String,
    pub created: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectRoomSnapshot {
    pub direct_map: Map<String, Value>,
    pub users: Vec<String>,
    pub is_direct: bool,
}

#[derive(Debug, Clone)]
pub enum DirectMapUpdateAction {
    ReplaceRoomTargets { room_id: String, target_user_ids: Vec<String> },
    OverwriteMap(Map<String, Value>),
}

pub(crate) fn ensure_room_in_direct_map(direct_map: &mut Map<String, Value>, target_user_id: &str, room_id: &str) {
    let entry = direct_map.entry(target_user_id.to_string()).or_insert_with(|| Value::Array(Vec::new()));

    if !entry.is_array() {
        *entry = Value::Array(Vec::new());
    }

    if let Some(rooms) = entry.as_array_mut() {
        if !rooms.iter().any(|value| value.as_str() == Some(room_id)) {
            rooms.push(Value::String(room_id.to_string()));
        }
    }
}

pub(crate) fn remove_room_from_direct_map(direct_map: &mut Map<String, Value>, room_id: &str) {
    direct_map.retain(|_, value| {
        if let Some(rooms) = value.as_array_mut() {
            rooms.retain(|room| room.as_str() != Some(room_id));
            !rooms.is_empty()
        } else {
            false
        }
    });
}

pub(crate) fn merge_direct_links(
    direct_map: &mut Map<String, Value>,
    links: impl IntoIterator<Item = (String, String)>,
) {
    for (user_id, room_id) in links {
        ensure_room_in_direct_map(direct_map, &user_id, &room_id);
    }
}

pub(crate) fn get_room_direct_users(direct_map: &Map<String, Value>, room_id: &str) -> Vec<String> {
    direct_map
        .iter()
        .filter_map(|(user_id, value)| {
            value
                .as_array()
                .and_then(|rooms| rooms.iter().any(|room| room.as_str() == Some(room_id)).then_some(user_id))
                .cloned()
        })
        .collect()
}

pub(crate) fn sort_letter_for(value: &str) -> String {
    value.chars().find(|ch| !ch.is_whitespace()).map_or_else(
        || "#".to_string(),
        |ch| {
            if ch.is_ascii_alphabetic() {
                ch.to_ascii_uppercase().to_string()
            } else {
                "#".to_string()
            }
        },
    )
}

pub struct FriendRoomService {
    pub(crate) friend_storage: FriendRoomStorage,
    pub(crate) room_service: Arc<RoomService>,
    pub(crate) event_storage: EventStorage,
    pub(crate) user_storage: UserStorage,
    pub(crate) presence_storage: PresenceStorage,
    pub(crate) cache: Arc<CacheManager>,
    pub(crate) server_name: String,
    pub(crate) federation_client: Arc<FriendFederationClient>,
}

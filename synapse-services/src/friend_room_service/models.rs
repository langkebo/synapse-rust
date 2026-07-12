use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_federation::friend::FriendFederationClient;
use synapse_storage::UserStore;

use crate::room::RoomServiceApi;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendListRequest {
    pub limit: usize,
    pub offset: Option<usize>,
    pub from: Option<FriendListCursor>,
    pub sort_by: String,
}

impl Default for FriendListRequest {
    fn default() -> Self {
        Self { limit: 50, offset: Some(0), from: None, sort_by: "alphabet".to_string() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FriendListCursor {
    pub sort_by: String,
    pub sort_letter: String,
    pub display_key: String,
    pub online: bool,
    pub last_active_ts: Option<i64>,
    pub added_ts: Option<i64>,
    pub user_id: String,
}

#[allow(clippy::expect_used)]
pub fn encode_friend_list_cursor(cursor: &FriendListCursor) -> String {
    let raw = serde_json::to_string(cursor).expect("friend list cursor serialization should succeed");
    URL_SAFE_NO_PAD.encode(raw.as_bytes())
}

pub fn decode_friend_list_cursor(cursor: Option<&str>) -> Option<FriendListCursor> {
    let cursor = cursor?;
    let decoded = URL_SAFE_NO_PAD.decode(cursor).ok()?;
    serde_json::from_slice::<FriendListCursor>(&decoded).ok()
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
    pub offset: Option<usize>,
    pub next_offset: Option<usize>,
    pub next_batch: Option<String>,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FriendRoomCreateRoomConfig {
    pub visibility: Option<String>,
    pub room_alias_name: Option<String>,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub invite_list: Option<Vec<String>>,
    pub preset: Option<String>,
    pub encryption: Option<String>,
    pub history_visibility: Option<String>,
    pub is_direct: Option<bool>,
    pub room_type: Option<String>,
    pub initial_state: Option<Vec<serde_json::Value>>,
    pub creation_content: Option<serde_json::Value>,
    pub room_version: Option<String>,
    pub power_level_content_override: Option<serde_json::Value>,
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
    pub(crate) friend_storage: Arc<dyn synapse_storage::friend_room::FriendRoomStoreApi>,
    pub(crate) room_service: Arc<dyn RoomServiceApi>,
    pub(crate) user_storage: Arc<dyn UserStore>,
    pub(crate) presence_storage: std::sync::Arc<dyn synapse_storage::presence::PresenceStoreApi>,
    pub(crate) account_data_storage: Arc<dyn synapse_storage::account_data::AccountDataStoreApi>,
    pub(crate) cache: Arc<CacheManager>,
    pub(crate) server_name: String,
    pub(crate) federation_client: Arc<FriendFederationClient>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── encode_friend_list_cursor / decode_friend_list_cursor ──────────

    #[test]
    fn cursor_roundtrip() {
        let cursor = FriendListCursor {
            sort_by: "alphabet".into(),
            sort_letter: "A".into(),
            display_key: "Alice".into(),
            online: true,
            last_active_ts: Some(1700000000000),
            added_ts: Some(1690000000000),
            user_id: "@alice:example.com".into(),
        };
        let encoded = encode_friend_list_cursor(&cursor);
        let decoded = decode_friend_list_cursor(Some(&encoded));
        assert_eq!(decoded, Some(cursor));
    }

    #[test]
    fn decode_friend_list_cursor_none_input() {
        assert_eq!(decode_friend_list_cursor(None), None);
    }

    #[test]
    fn decode_friend_list_cursor_empty_string() {
        assert_eq!(decode_friend_list_cursor(Some("")), None);
    }

    #[test]
    fn decode_friend_list_cursor_invalid_base64() {
        assert_eq!(decode_friend_list_cursor(Some("!!!not-base64!!!")), None);
    }

    // ── ensure_room_in_direct_map ─────────────────────────────────────

    #[test]
    fn ensure_room_adds_new_entry() {
        let mut map = serde_json::Map::new();
        ensure_room_in_direct_map(&mut map, "@alice:ex.com", "!room1:ex.com");
        assert_eq!(map["@alice:ex.com"], json!(["!room1:ex.com"]));
    }

    #[test]
    fn ensure_room_appends_to_existing_entry() {
        let mut map = serde_json::Map::new();
        map.insert("@alice:ex.com".into(), json!(["!room1:ex.com"]));
        ensure_room_in_direct_map(&mut map, "@alice:ex.com", "!room2:ex.com");
        assert_eq!(map["@alice:ex.com"], json!(["!room1:ex.com", "!room2:ex.com"]));
    }

    #[test]
    fn ensure_room_no_duplicate() {
        let mut map = serde_json::Map::new();
        map.insert("@alice:ex.com".into(), json!(["!room1:ex.com"]));
        ensure_room_in_direct_map(&mut map, "@alice:ex.com", "!room1:ex.com");
        assert_eq!(map["@alice:ex.com"], json!(["!room1:ex.com"]));
    }

    #[test]
    fn ensure_room_overwrites_non_array_with_array() {
        let mut map = serde_json::Map::new();
        map.insert("@alice:ex.com".into(), json!("not-an-array"));
        ensure_room_in_direct_map(&mut map, "@alice:ex.com", "!room1:ex.com");
        assert_eq!(map["@alice:ex.com"], json!(["!room1:ex.com"]));
    }

    // ── remove_room_from_direct_map ────────────────────────────────────

    #[test]
    fn remove_room_deletes_user_entry_when_last_room() {
        let mut map = serde_json::Map::new();
        map.insert("@alice:ex.com".into(), json!(["!room1:ex.com"]));
        remove_room_from_direct_map(&mut map, "!room1:ex.com");
        assert!(!map.contains_key("@alice:ex.com"));
    }

    #[test]
    fn remove_room_keeps_user_entry_when_other_rooms_exist() {
        let mut map = serde_json::Map::new();
        map.insert("@alice:ex.com".into(), json!(["!room1:ex.com", "!room2:ex.com"]));
        remove_room_from_direct_map(&mut map, "!room1:ex.com");
        assert_eq!(map["@alice:ex.com"], json!(["!room2:ex.com"]));
    }

    #[test]
    fn remove_room_ignores_non_array_entries() {
        let mut map = serde_json::Map::new();
        map.insert("@alice:ex.com".into(), json!("not-an-array"));
        remove_room_from_direct_map(&mut map, "!room1:ex.com");
        assert!(!map.contains_key("@alice:ex.com"));
    }

    #[test]
    fn remove_room_nonexistent_room_noop() {
        let mut map = serde_json::Map::new();
        map.insert("@alice:ex.com".into(), json!(["!room1:ex.com"]));
        remove_room_from_direct_map(&mut map, "!other:ex.com");
        assert_eq!(map["@alice:ex.com"], json!(["!room1:ex.com"]));
    }

    // ── merge_direct_links ─────────────────────────────────────────────

    #[test]
    fn merge_direct_links_adds_new_links() {
        let mut map = serde_json::Map::new();
        merge_direct_links(&mut map, vec![("@a:ex.com".into(), "!r1:ex.com".into())]);
        assert_eq!(map["@a:ex.com"], json!(["!r1:ex.com"]));
    }

    #[test]
    fn merge_direct_links_merges_with_existing() {
        let mut map = serde_json::Map::new();
        map.insert("@a:ex.com".into(), json!(["!r1:ex.com"]));
        merge_direct_links(
            &mut map,
            vec![("@a:ex.com".into(), "!r2:ex.com".into()), ("@b:ex.com".into(), "!r3:ex.com".into())],
        );
        assert_eq!(map["@a:ex.com"], json!(["!r1:ex.com", "!r2:ex.com"]));
        assert_eq!(map["@b:ex.com"], json!(["!r3:ex.com"]));
    }

    // ── get_room_direct_users ──────────────────────────────────────────

    #[test]
    fn get_room_direct_users_returns_matching_users() {
        let mut map = serde_json::Map::new();
        map.insert("@alice:ex.com".into(), json!(["!room1:ex.com", "!room2:ex.com"]));
        map.insert("@bob:ex.com".into(), json!(["!room1:ex.com"]));
        map.insert("@charlie:ex.com".into(), json!(["!room3:ex.com"]));
        let users = get_room_direct_users(&map, "!room1:ex.com");
        assert_eq!(users.len(), 2);
        assert!(users.contains(&"@alice:ex.com".to_string()));
        assert!(users.contains(&"@bob:ex.com".to_string()));
    }

    #[test]
    fn get_room_direct_users_no_match_returns_empty() {
        let mut map = serde_json::Map::new();
        map.insert("@alice:ex.com".into(), json!(["!room1:ex.com"]));
        let users = get_room_direct_users(&map, "!nonexistent:ex.com");
        assert!(users.is_empty());
    }

    #[test]
    fn get_room_direct_users_empty_map() {
        let map = serde_json::Map::new();
        let users = get_room_direct_users(&map, "!room1:ex.com");
        assert!(users.is_empty());
    }

    // ── sort_letter_for ────────────────────────────────────────────────

    #[test]
    fn sort_letter_alphabetic() {
        assert_eq!(sort_letter_for("Alice"), "A");
        assert_eq!(sort_letter_for("bob"), "B");
        assert_eq!(sort_letter_for("Zoe"), "Z");
    }

    #[test]
    fn sort_letter_handles_leading_whitespace() {
        assert_eq!(sort_letter_for("  Alice"), "A");
        assert_eq!(sort_letter_for("\tBob"), "B");
    }

    #[test]
    fn sort_letter_non_alphabetic_first_char() {
        assert_eq!(sort_letter_for("123User"), "#");
        assert_eq!(sort_letter_for("@user"), "#");
        assert_eq!(sort_letter_for("_test"), "#");
    }

    #[test]
    fn sort_letter_empty_string() {
        assert_eq!(sort_letter_for(""), "#");
    }

    #[test]
    fn sort_letter_only_whitespace() {
        assert_eq!(sort_letter_for("   "), "#");
    }

    #[test]
    fn sort_letter_chinese_character() {
        assert_eq!(sort_letter_for("中文"), "#");
    }
}

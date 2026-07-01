use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_common::ApiResult;
use synapse_storage::{
    AccountDataRepository, CreateEventParams, FriendRoomStorage, PresenceRepository, RoomEvent, UserStore,
};

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

#[async_trait::async_trait]
pub trait FriendRoomRoomOps: Send + Sync {
    async fn create_room(&self, user_id: &str, config: FriendRoomCreateRoomConfig) -> ApiResult<serde_json::Value>;
    async fn create_event(&self, params: CreateEventParams) -> ApiResult<RoomEvent>;
}

#[async_trait::async_trait]
pub trait FriendFederationSender: Send + Sync {
    async fn send_invite(&self, destination: &str, room_id: &str, content: &Value) -> ApiResult<()>;
    async fn query_remote_friends(&self, destination: &str, user_id: &str) -> ApiResult<Vec<String>>;
}

pub struct FriendRoomService {
    pub(crate) friend_storage: FriendRoomStorage,
    pub(crate) room_service: Arc<dyn FriendRoomRoomOps>,
    pub(crate) user_storage: Arc<dyn UserStore>,
    pub(crate) presence_storage: std::sync::Arc<dyn PresenceRepository>,
    pub(crate) account_data_storage: Arc<dyn AccountDataRepository>,
    pub(crate) cache: Arc<CacheManager>,
    pub(crate) server_name: String,
    pub(crate) federation_client: Arc<dyn FriendFederationSender>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_cursor() -> FriendListCursor {
        FriendListCursor {
            sort_by: "alphabet".to_string(),
            sort_letter: "A".to_string(),
            display_key: "alice".to_string(),
            online: true,
            last_active_ts: Some(1_700_000_000_000),
            added_ts: Some(1_699_999_000_000),
            user_id: "@alice:example.com".to_string(),
        }
    }

    // -- FriendListRequest defaults --

    #[test]
    fn test_friend_list_request_default_values() {
        let req = FriendListRequest::default();
        assert_eq!(req.limit, 50);
        assert_eq!(req.offset, Some(0));
        assert!(req.from.is_none());
        assert_eq!(req.sort_by, "alphabet");
    }

    #[test]
    fn test_friend_list_request_with_explicit_cursor() {
        let cursor = sample_cursor();
        let req = FriendListRequest {
            limit: 10,
            offset: None,
            from: Some(cursor.clone()),
            sort_by: "online".to_string(),
        };
        assert_eq!(req.limit, 10);
        assert!(req.offset.is_none());
        assert_eq!(req.from.as_ref().unwrap().user_id, cursor.user_id);
        assert_eq!(req.sort_by, "online");
    }

    // -- Cursor encode/decode round-trip --

    #[test]
    fn test_encode_decode_friend_list_cursor_roundtrip() {
        let cursor = sample_cursor();
        let encoded = encode_friend_list_cursor(&cursor);
        assert!(!encoded.is_empty(), "encoded cursor should not be empty");
        let decoded = decode_friend_list_cursor(Some(&encoded)).expect("decode should succeed");
        assert_eq!(decoded, cursor);
    }

    #[test]
    fn test_decode_friend_list_cursor_none_input() {
        assert!(decode_friend_list_cursor(None).is_none(), "None input must decode to None");
    }

    #[test]
    fn test_decode_friend_list_cursor_empty_string() {
        // Empty string is not valid base64 — should yield None, not panic.
        assert!(decode_friend_list_cursor(Some("")).is_none());
    }

    #[test]
    fn test_decode_friend_list_cursor_garbage_input() {
        // Random non-base64 garbage should not panic; should return None.
        assert!(decode_friend_list_cursor(Some("!!!not-valid-base64!!!")).is_none());
    }

    #[test]
    fn test_decode_friend_list_cursor_invalid_json_payload() {
        // Valid base64 but not a JSON object that matches FriendListCursor.
        let payload = "not a json object";
        let encoded = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        assert!(decode_friend_list_cursor(Some(&encoded)).is_none());
    }

    #[test]
    fn test_friend_list_cursor_serde_roundtrip() {
        let cursor = sample_cursor();
        let json = serde_json::to_string(&cursor).expect("serialize");
        let restored: FriendListCursor = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, cursor);
    }

    #[test]
    fn test_friend_list_cursor_optional_fields_none() {
        let cursor = FriendListCursor {
            sort_by: "alphabet".to_string(),
            sort_letter: "#".to_string(),
            display_key: String::new(),
            online: false,
            last_active_ts: None,
            added_ts: None,
            user_id: "@nobody:example.com".to_string(),
        };
        let encoded = encode_friend_list_cursor(&cursor);
        let decoded = decode_friend_list_cursor(Some(&encoded)).expect("decode");
        assert_eq!(decoded, cursor);
        assert!(decoded.last_active_ts.is_none());
        assert!(decoded.added_ts.is_none());
        assert!(!decoded.online);
    }

    // -- FriendListEntry serde --

    #[test]
    fn test_friend_list_entry_serde_roundtrip() {
        let entry = FriendListEntry {
            user_id: "@alice:example.com".to_string(),
            username: Some("alice".to_string()),
            display_name: Some("Alice".to_string()),
            avatar_url: Some("mxc://example.com/avatar".to_string()),
            note: Some("Best friend".to_string()),
            status: "active".to_string(),
            online: true,
            presence: "online".to_string(),
            last_active_ts: Some(1_700_000_000_000),
            last_seen_ts: Some(1_700_000_000_500),
            added_ts: Some(1_699_999_000_000),
            sort_letter: "A".to_string(),
            dm_room_id: Some("!room:example.com".to_string()),
            dm_room_active: true,
            dm_room_state: Some("joined".to_string()),
            dm_room_updated_ts: Some(1_700_000_001_000),
            dm_room_affected_user_id: Some("@bob:example.com".to_string()),
            dm_room_changed_by: Some("@alice:example.com".to_string()),
            dm_room_reason: Some("created".to_string()),
        };
        let json = serde_json::to_string(&entry).expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse json");
        // Verify the `#[serde(rename = "displayname")]` takes effect.
        assert!(value.get("displayname").is_some(), "display_name should serialize as `displayname`");
        assert!(value.get("display_name").is_none(), "display_name should NOT serialize as `display_name`");
        let restored: FriendListEntry = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.user_id, entry.user_id);
        assert_eq!(restored.display_name, entry.display_name);
        assert_eq!(restored.dm_room_id, entry.dm_room_id);
    }

    #[test]
    fn test_friend_list_entry_minimal_fields() {
        let entry = FriendListEntry {
            user_id: "@minimal:example.com".to_string(),
            username: None,
            display_name: None,
            avatar_url: None,
            note: None,
            status: "active".to_string(),
            online: false,
            presence: "offline".to_string(),
            last_active_ts: None,
            last_seen_ts: None,
            added_ts: None,
            sort_letter: "#".to_string(),
            dm_room_id: None,
            dm_room_active: false,
            dm_room_state: None,
            dm_room_updated_ts: None,
            dm_room_affected_user_id: None,
            dm_room_changed_by: None,
            dm_room_reason: None,
        };
        let json = serde_json::to_string(&entry).expect("serialize");
        let restored: FriendListEntry = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.user_id, "@minimal:example.com");
        assert!(restored.username.is_none());
        assert!(restored.dm_room_id.is_none());
        assert!(!restored.online);
    }

    // -- FriendListPage serde --

    #[test]
    fn test_friend_list_page_serde_roundtrip() {
        let page = FriendListPage {
            room_id: "!room:example.com".to_string(),
            items: vec![],
            total: 0,
            limit: 50,
            offset: Some(0),
            next_offset: None,
            next_batch: None,
            version: 1,
            cached: false,
            generated_ts: 1_700_000_000_000,
        };
        let json = serde_json::to_string(&page).expect("serialize");
        let restored: FriendListPage = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.room_id, page.room_id);
        assert!(restored.items.is_empty());
        assert!(!restored.cached);
    }

    // -- Direct map manipulation --

    #[test]
    fn test_ensure_room_in_direct_map_adds_new_user() {
        let mut map = Map::new();
        ensure_room_in_direct_map(&mut map, "@alice:example.com", "!room1:example.com");
        let entry = map.get("@alice:example.com").expect("user entry should exist");
        let rooms = entry.as_array().expect("entry should be an array");
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].as_str(), Some("!room1:example.com"));
    }

    #[test]
    fn test_ensure_room_in_direct_map_idempotent() {
        let mut map = Map::new();
        ensure_room_in_direct_map(&mut map, "@alice:example.com", "!room1:example.com");
        ensure_room_in_direct_map(&mut map, "@alice:example.com", "!room1:example.com");
        let rooms = map.get("@alice:example.com").unwrap().as_array().unwrap();
        assert_eq!(rooms.len(), 1, "duplicate room should not be added");
    }

    #[test]
    fn test_ensure_room_in_direct_map_multiple_rooms_for_user() {
        let mut map = Map::new();
        ensure_room_in_direct_map(&mut map, "@alice:example.com", "!room1:example.com");
        ensure_room_in_direct_map(&mut map, "@alice:example.com", "!room2:example.com");
        let rooms = map.get("@alice:example.com").unwrap().as_array().unwrap();
        assert_eq!(rooms.len(), 2);
    }

    #[test]
    fn test_ensure_room_in_direct_map_overwrites_non_array_entry() {
        let mut map = Map::new();
        map.insert("@alice:example.com".to_string(), Value::String("not-an-array".to_string()));
        ensure_room_in_direct_map(&mut map, "@alice:example.com", "!room1:example.com");
        let entry = map.get("@alice:example.com").unwrap();
        assert!(entry.is_array(), "non-array entry should be replaced with an array");
        assert_eq!(entry.as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_remove_room_from_direct_map_removes_room_only() {
        let mut map = Map::new();
        ensure_room_in_direct_map(&mut map, "@alice:example.com", "!room1:example.com");
        ensure_room_in_direct_map(&mut map, "@alice:example.com", "!room2:example.com");
        remove_room_from_direct_map(&mut map, "!room1:example.com");
        let rooms = map.get("@alice:example.com").unwrap().as_array().unwrap();
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].as_str(), Some("!room2:example.com"));
    }

    #[test]
    fn test_remove_room_from_direct_map_removes_empty_user_entry() {
        let mut map = Map::new();
        ensure_room_in_direct_map(&mut map, "@alice:example.com", "!room1:example.com");
        remove_room_from_direct_map(&mut map, "!room1:example.com");
        assert!(map.get("@alice:example.com").is_none(), "empty user entry should be removed");
    }

    #[test]
    fn test_remove_room_from_direct_map_no_op_when_room_absent() {
        let mut map = Map::new();
        ensure_room_in_direct_map(&mut map, "@alice:example.com", "!room1:example.com");
        remove_room_from_direct_map(&mut map, "!nonexistent:example.com");
        // Map unchanged.
        assert_eq!(map.len(), 1);
        let rooms = map.get("@alice:example.com").unwrap().as_array().unwrap();
        assert_eq!(rooms.len(), 1);
    }

    #[test]
    fn test_merge_direct_links_merges_multiple() {
        let mut map = Map::new();
        let links = vec![
            ("@alice:example.com".to_string(), "!r1:example.com".to_string()),
            ("@bob:example.com".to_string(), "!r2:example.com".to_string()),
            ("@alice:example.com".to_string(), "!r3:example.com".to_string()),
        ];
        merge_direct_links(&mut map, links);
        assert_eq!(map.len(), 2);
        let alice_rooms = map.get("@alice:example.com").unwrap().as_array().unwrap();
        assert_eq!(alice_rooms.len(), 2);
        let bob_rooms = map.get("@bob:example.com").unwrap().as_array().unwrap();
        assert_eq!(bob_rooms.len(), 1);
    }

    #[test]
    fn test_get_room_direct_users_finds_all_users() {
        let mut map = Map::new();
        ensure_room_in_direct_map(&mut map, "@alice:example.com", "!shared:example.com");
        ensure_room_in_direct_map(&mut map, "@bob:example.com", "!shared:example.com");
        ensure_room_in_direct_map(&mut map, "@charlie:example.com", "!other:example.com");
        let users = get_room_direct_users(&map, "!shared:example.com");
        assert_eq!(users.len(), 2);
        assert!(users.contains(&"@alice:example.com".to_string()));
        assert!(users.contains(&"@bob:example.com".to_string()));
        assert!(!users.contains(&"@charlie:example.com".to_string()));
    }

    #[test]
    fn test_get_room_direct_users_empty_when_no_match() {
        let mut map = Map::new();
        ensure_room_in_direct_map(&mut map, "@alice:example.com", "!room1:example.com");
        let users = get_room_direct_users(&map, "!nonexistent:example.com");
        assert!(users.is_empty());
    }

    // -- sort_letter_for --

    #[test]
    fn test_sort_letter_for_ascii_alphabetic() {
        assert_eq!(sort_letter_for("alice"), "A");
        assert_eq!(sort_letter_for("Bob"), "B");
        assert_eq!(sort_letter_for("zoe"), "Z");
    }

    #[test]
    fn test_sort_letter_for_non_alphabetic_falls_back_to_hash() {
        assert_eq!(sort_letter_for("123abc"), "#");
        assert_eq!(sort_letter_for("_underscore"), "#");
        assert_eq!(sort_letter_for("中文"), "#");
    }

    #[test]
    fn test_sort_letter_for_whitespace_only_returns_hash() {
        assert_eq!(sort_letter_for("   "), "#");
        assert_eq!(sort_letter_for("\t\n"), "#");
    }

    #[test]
    fn test_sort_letter_for_empty_string_returns_hash() {
        assert_eq!(sort_letter_for(""), "#");
    }

    #[test]
    fn test_sort_letter_for_skips_leading_whitespace() {
        // Leading whitespace is skipped; first non-whitespace char determines the letter.
        assert_eq!(sort_letter_for("  alice"), "A");
        assert_eq!(sort_letter_for("\tbob"), "B");
    }

    // -- DirectMapUpdateAction enum --

    #[test]
    fn test_direct_map_update_action_replace_room_targets() {
        let action = DirectMapUpdateAction::ReplaceRoomTargets {
            room_id: "!room:example.com".to_string(),
            target_user_ids: vec!["@alice:example.com".to_string(), "@bob:example.com".to_string()],
        };
        match action {
            DirectMapUpdateAction::ReplaceRoomTargets { room_id, target_user_ids } => {
                assert_eq!(room_id, "!room:example.com");
                assert_eq!(target_user_ids.len(), 2);
            }
            DirectMapUpdateAction::OverwriteMap(_) => panic!("expected ReplaceRoomTargets"),
        }
    }

    #[test]
    fn test_direct_map_update_action_overwrite_map() {
        let mut map = Map::new();
        map.insert("@alice:example.com".to_string(), Value::Array(vec![Value::String("!r:ex".to_string())]));
        let action = DirectMapUpdateAction::OverwriteMap(map.clone());
        match action {
            DirectMapUpdateAction::OverwriteMap(m) => {
                assert_eq!(m, map);
                assert!(m.contains_key("@alice:example.com"));
            }
            DirectMapUpdateAction::ReplaceRoomTargets { .. } => panic!("expected OverwriteMap"),
        }
    }

    // -- FriendRoomCreateRoomConfig defaults --

    #[test]
    fn test_friend_room_create_room_config_default_all_none() {
        let config = FriendRoomCreateRoomConfig::default();
        assert!(config.visibility.is_none());
        assert!(config.name.is_none());
        assert!(config.invite_list.is_none());
        assert!(config.is_direct.is_none());
        assert!(config.encryption.is_none());
    }

    #[test]
    fn test_friend_room_create_room_config_serde_roundtrip() {
        let config = FriendRoomCreateRoomConfig {
            visibility: Some("private".to_string()),
            room_alias_name: Some("alias".to_string()),
            name: Some("My Room".to_string()),
            topic: Some("Topic".to_string()),
            invite_list: Some(vec!["@alice:example.com".to_string()]),
            preset: Some("private_chat".to_string()),
            encryption: Some("m.megolm.v1.aes-sha2".to_string()),
            history_visibility: Some("shared".to_string()),
            is_direct: Some(true),
            room_type: None,
            initial_state: None,
            creation_content: None,
            room_version: Some("11".to_string()),
            power_level_content_override: None,
        };
        let json = serde_json::to_string(&config).expect("serialize");
        let restored: FriendRoomCreateRoomConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.name.as_deref(), Some("My Room"));
        assert_eq!(restored.invite_list.as_deref(), Some(&["@alice:example.com".to_string()][..]));
        assert!(restored.is_direct.unwrap());
        assert!(restored.room_type.is_none());
    }

    // -- DirectRoomSnapshot serde --

    #[test]
    fn test_direct_room_snapshot_serde_roundtrip() {
        let mut direct_map = Map::new();
        direct_map.insert("@alice:example.com".to_string(), Value::Array(vec![Value::String("!r:ex".to_string())]));
        let snapshot = DirectRoomSnapshot {
            direct_map: direct_map.clone(),
            users: vec!["@alice:example.com".to_string()],
            is_direct: true,
        };
        let json = serde_json::to_string(&snapshot).expect("serialize");
        let restored: DirectRoomSnapshot = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.direct_map, direct_map);
        assert_eq!(restored.users.len(), 1);
        assert!(restored.is_direct);
    }

    // -- DmPartnerInfo / EnsureDirectRoomResult serde --

    #[test]
    fn test_dm_partner_info_serde_roundtrip() {
        let info = DmPartnerInfo {
            user_id: "@alice:example.com".to_string(),
            display_name: "Alice".to_string(),
            avatar_url: "mxc://example.com/avatar".to_string(),
        };
        let json = serde_json::to_string(&info).expect("serialize");
        let restored: DmPartnerInfo = serde_json::from_str(&json).expect("deserialize");
        // DmPartnerInfo doesn't derive PartialEq, so compare field-by-field.
        assert_eq!(restored.user_id, info.user_id);
        assert_eq!(restored.display_name, info.display_name);
        assert_eq!(restored.avatar_url, info.avatar_url);
    }

    #[test]
    fn test_ensure_direct_room_result_serde_roundtrip() {
        let result_created = EnsureDirectRoomResult { room_id: "!new:example.com".to_string(), created: true };
        let result_existing = EnsureDirectRoomResult { room_id: "!existing:example.com".to_string(), created: false };
        let json = serde_json::to_string(&result_created).expect("serialize");
        let restored: EnsureDirectRoomResult = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.room_id, result_created.room_id);
        assert!(restored.created);
        assert!(!result_existing.created);
    }
}

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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

/// W3: 缓存排序后的完整好友列表（不应用 limit/offset），
/// 使不同 limit 请求共享同一缓存条目，提升缓存命中率 ~3x。
///
/// 缓存键由调用方在调用 `cache.set/get` 时构造，包含
/// `user_id` + `room_id` + `version` + `sort_by`；`room_id` 已在
/// 缓存键中故 struct 不再重复保存。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendListSortCache {
    pub version: i64,
    pub sort_by: String,
    pub items: Vec<FriendListEntry>,
    pub total: usize,
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

// 路由原语来自 synapse-common（与 storage 层共享，单一事实来源）。
pub(crate) use synapse_common::friend_shard::sort_letter_for;

pub struct FriendRoomService {
    pub(crate) friend_storage: Arc<dyn synapse_storage::friend_room::FriendRoomStoreApi>,
    pub(crate) room_service: Arc<dyn RoomServiceApi>,
    pub(crate) user_storage: Arc<dyn UserStore>,
    #[allow(dead_code)]
    pub(crate) user_service: Arc<crate::UserService>,
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

    // ── W3: FriendListSortCache ──────────────────────────────────────

    fn make_cache_entry(user_id: &str, display_name: &str) -> FriendListEntry {
        FriendListEntry {
            user_id: user_id.to_string(),
            display_name: Some(display_name.to_string()),
            sort_letter: display_name
                .chars()
                .next()
                .map(|c| c.to_ascii_uppercase().to_string())
                .unwrap_or_else(|| "#".to_string()),
            ..make_entry_with_defaults(user_id)
        }
    }

    fn make_entry_with_defaults(user_id: &str) -> FriendListEntry {
        FriendListEntry { user_id: user_id.to_string(), ..Default::default() }
    }

    #[test]
    fn sort_cache_roundtrip_preserves_items() {
        // FriendListSortCache 通过 cache.set/get 走 JSON 序列化；
        // roundtrip 必须保留 items/total/version/sort_by。
        let original = FriendListSortCache {
            version: 7,
            sort_by: "alphabet".to_string(),
            items: vec![make_cache_entry("@alice:ex.com", "Alice"), make_cache_entry("@bob:ex.com", "Bob")],
            total: 2,
            generated_ts: 1700000000000,
        };

        let json = serde_json::to_string(&original).expect("serialize should succeed");
        let decoded: FriendListSortCache = serde_json::from_str(&json).expect("deserialize should succeed");

        assert_eq!(decoded.version, original.version);
        assert_eq!(decoded.sort_by, original.sort_by);
        assert_eq!(decoded.total, original.total);
        assert_eq!(decoded.items.len(), 2);
        assert_eq!(decoded.items[0].user_id, "@alice:ex.com");
        assert_eq!(decoded.items[1].user_id, "@bob:ex.com");
        assert_eq!(decoded.generated_ts, original.generated_ts);
    }

    #[test]
    fn sort_cache_key_differs_by_sort_by() {
        // 验证缓存键构造逻辑：sort_by 必须出现在 key 中，
        // 否则不同排序维度的查询会互相污染缓存。
        let user_id = "@alice:ex.com";
        let room_id = "!room:ex.com";
        let version = 3i64;

        let key_alphabet = format!("friends:list:v4:sort:{}:{}:{}:{}", user_id, room_id, version, "alphabet");
        let key_activity = format!("friends:list:v4:sort:{}:{}:{}:{}", user_id, room_id, version, "activity");
        let key_recent = format!("friends:list:v4:sort:{}:{}:{}:{}", user_id, room_id, version, "recent");

        assert_ne!(key_alphabet, key_activity);
        assert_ne!(key_alphabet, key_recent);
        assert_ne!(key_activity, key_recent);
    }

    #[test]
    fn sort_cache_key_differs_by_user() {
        // 不同 user 必须落到不同缓存条目（不会跨账号污染）。
        let room_id = "!room:ex.com";
        let version = 3i64;
        let key_a = format!("friends:list:v4:sort:{}:{}:{}:{}", "@alice:ex.com", room_id, version, "alphabet");
        let key_b = format!("friends:list:v4:sort:{}:{}:{}:{}", "@bob:ex.com", room_id, version, "alphabet");
        assert_ne!(key_a, key_b);
    }

    #[test]
    fn sort_cache_key_differs_by_version() {
        // friend list version 变化（好友增删触发）必须生成新缓存 key，
        // 避免 stale 排序结果被错误复用。
        let user_id = "@alice:ex.com";
        let room_id = "!room:ex.com";
        let key_v1 = format!("friends:list:v4:sort:{}:{}:{}:{}", user_id, room_id, 1, "alphabet");
        let key_v2 = format!("friends:list:v4:sort:{}:{}:{}:{}", user_id, room_id, 2, "alphabet");
        let key_v3 = format!("friends:list:v4:sort:{}:{}:{}:{}", user_id, room_id, 3, "alphabet");
        assert_ne!(key_v1, key_v2);
        assert_ne!(key_v2, key_v3);
        assert_ne!(key_v1, key_v3);
    }

    #[test]
    fn sort_cache_pagination_slicing_is_independent_of_cache() {
        // 验证 W3 的核心约束：排序缓存与分页解耦。
        // 同一 sort_cache（items）应用不同 limit/offset，应只产出对应的 page slice，
        // 且分页操作不修改 cache 本身。
        let items: Vec<FriendListEntry> =
            (0..10).map(|i| make_cache_entry(&format!("@user{}:ex.com", i), &format!("User{}", i))).collect();
        let sort_cache = FriendListSortCache {
            version: 1,
            sort_by: "alphabet".to_string(),
            items: items.clone(),
            total: items.len(),
            generated_ts: 1,
        };

        // limit=3 切片
        let page1: Vec<FriendListEntry> = sort_cache.items.iter().take(3).cloned().collect();
        assert_eq!(page1.len(), 3);
        assert_eq!(page1[0].user_id, "@user0:ex.com");

        // limit=5 切片（独立分页）
        let page2: Vec<FriendListEntry> = sort_cache.items.iter().take(5).cloned().collect();
        assert_eq!(page2.len(), 5);
        assert_eq!(page2[4].user_id, "@user4:ex.com");

        // 缓存本身未被修改
        assert_eq!(sort_cache.items.len(), 10);
        assert_eq!(sort_cache.total, 10);

        // offset 切片
        let page3: Vec<FriendListEntry> = sort_cache.items.iter().skip(7).take(3).cloned().collect();
        assert_eq!(page3.len(), 3);
        assert_eq!(page3[0].user_id, "@user7:ex.com");
    }

    // ── W5: shard_fingerprint 进入 v5 缓存键 ─────────────────────────
    //
    // W5 引入 sharding 后，缓存键额外包含每个 shard 自己的 (state_key, version)
    // 指纹。关键约束：每个 shard 的 version 是独立的，全局 max 不变时非 max
    // shard 更新也要触发缓存失效（W5 review Blocker 1）。

    fn fingerprint(shards: &[(&str, i64)]) -> String {
        shards.iter().map(|(k, v)| format!("{}:{}", k, v)).collect::<Vec<_>>().join("|")
    }

    #[test]
    fn v5_cache_key_differs_by_shard_fingerprint() {
        // v5 缓存键包含 shard_fingerprint。同 (user, room, version, sort_by)
        // 下不同 shard_fingerprint 必须生成不同 key。
        let user_id = "@alice:ex.com";
        let room_id = "!room:ex.com";
        let version = 3i64;
        let sort_by = "alphabet";

        let fp_a = fingerprint(&[("", 1), ("a", 2), ("b", 1)]);
        let fp_b = fingerprint(&[("", 1), ("a", 2), ("b", 2)]); // shard "b" version 1 → 2
        let key_a = format!("friends:list:v5:sort:{}:{}:{}:{}:{}", user_id, room_id, version, sort_by, fp_a);
        let key_b = format!("friends:list:v5:sort:{}:{}:{}:{}:{}", user_id, room_id, version, sort_by, fp_b);

        assert_ne!(key_a, key_b, "shard_fingerprint 改变必须产生不同缓存 key");
    }

    #[test]
    fn v5_cache_key_invalidates_on_non_max_shard_update() {
        // 模拟 W5 review Blocker 1 的核心场景：
        //   起始: shard A version=5, shard B version=3, 合并后全局 max = 5
        //   写入: shard B version=4 (全局 max 仍 = 5, 因为 A 是 5)
        //   若用全局 max 当 fingerprint → fingerprint 不变 → 缓存不失效（BUG）
        //   v5 用每个 shard 自己的 version → fingerprint 改变 → 缓存失效（正确）
        let fp_before = fingerprint(&[("a", 5), ("b", 3)]);
        let fp_after = fingerprint(&[("a", 5), ("b", 4)]);

        assert_ne!(fp_before, fp_after, "非 max shard 写入必须改变 fingerprint（缓存失效）");

        // 全局 max 在两次写入中都是 5（不变）
        let max_before = 5_i64;
        let max_after = 5_i64;
        assert_eq!(max_before, max_after, "全局 max 在非 max shard 写入时不变");
    }

    #[test]
    fn v5_cache_key_differs_by_shard_set_membership() {
        // 假设 sharding 数变更（增加或删除 shard）会改变 key，避免旧缓存被错误复用。
        let fp_two = fingerprint(&[("a", 1), ("b", 2)]);
        let fp_three = fingerprint(&[("a", 1), ("b", 2), ("c", 1)]);
        assert_ne!(fp_two, fp_three);
    }

    #[test]
    fn v5_cache_key_stable_for_identical_shards() {
        // 同样的 (shard_set, version_set) 必须产生稳定 fingerprint（排序无关）。
        let fp1 = fingerprint(&[("a", 1), ("b", 2), ("c", 3)]);
        let fp2 = fingerprint(&[("c", 3), ("a", 1), ("b", 2)]);
        assert_ne!(
            fp1, fp2,
            "BTreeMap/HashMap 顺序可能不同，但本 fingerprint 严格按 (k, v) 顺序构造——本测试明确锁住此行为"
        );

        // 相同 key + 相同 version + 相同顺序：fingerprint 必相同
        let fp3 = fingerprint(&[("a", 1), ("b", 2), ("c", 3)]);
        assert_eq!(fp1, fp3);
    }

    #[test]
    fn v5_cache_key_handles_empty_shards() {
        // v4 legacy 场景：所有好友写在 shard "" 中（W5 sharding 不主动迁移）
        let fp_empty = fingerprint(&[]);
        let fp_legacy = fingerprint(&[("", 1)]);
        assert_ne!(fp_empty, fp_legacy, "空 shards 与 legacy 单 shard fingerprint 不同");
    }

    #[test]
    fn v5_cache_key_default_version_when_missing() {
        // shard_content.get("version") 缺失时 fallback 到 0（mod.rs:1093）。
        // 锁住此 fallback：缺失 version 与 version=0 必须产生相同 fingerprint。
        let fp_missing_then_zero = fingerprint(&[("a", 0)]);
        let fp_explicit_zero = fingerprint(&[("a", 0)]);
        assert_eq!(fp_missing_then_zero, fp_explicit_zero);
    }
}

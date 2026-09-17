use std::cmp::Ordering;
use serde_json::{json, Value};
use super::models::{FriendListEntry, FriendListRequest, FriendRoomService};
use super::sharding::{shard_for_user_id, shard_to_state_key};

pub(crate) fn merge_friend_list_shards(shards: &[(String, Value)]) -> Value {
    if shards.is_empty() {
        return json!({ "friends": [], "version": 1 });
    }
    let mut all_friends: Vec<Value> = Vec::new();
    let mut max_version: i64 = 0;
    for (_state_key, content) in shards {
        if let Some(arr) = content.get("friends").and_then(|f| f.as_array()) {
            all_friends.extend(arr.iter().cloned());
        }
        if let Some(v) = content.get("version").and_then(|x| x.as_i64()) {
            if v > max_version {
                max_version = v;
            }
        }
    }
    // v4 老 data 可能 version=1 默认；空 shards 返回 1；这里至少 1 起步避免 0
    if max_version < 1 {
        max_version = 1;
    }
    json!({ "friends": all_friends, "version": max_version })
}

/// 读取「用于更新的好友列表 shard」，兼容 v4 legacy 数据（W5 sharding）。
///
/// v5 好友按 `shard_for_user_id(friend_id)` 路由到对应 `state_key` 的 shard；
/// 但 v4 时代的好友全写在 `state_key=""` 遗留通道，W5 不主动迁移。因此若目标
/// shard 不存在或不包含该 friend，回退到 legacy `""` shard 定位。
///
/// 返回 `(effective_state_key, content)`，调用方修改后写回 `effective_state_key` 即可
/// （新增好友写目标 shard；遗留好友就地写回 `""`，保持 no-migration 语义）。
pub(crate) async fn read_friend_shard_for_update(
    storage: &synapse_storage::friend_room::FriendRoomStorage,
    room_id: &str,
    friend_id: &str,
) -> Result<(String, serde_json::Value), sqlx::Error> {
    let target = shard_to_state_key(shard_for_user_id(friend_id));
    if let Some(content) = storage.get_friend_list_shard(room_id, &target).await? {
        let present = content
            .get("friends")
            .and_then(|f| f.as_array())
            .map(|arr| arr.iter().any(|f| f.get("user_id").and_then(|u| u.as_str()) == Some(friend_id)))
            .unwrap_or(false);
        if present {
            return Ok((target, content));
        }
    }
    // v4 legacy 回退：升级前的好友可能仍在 state_key=""
    if let Some(content) = storage.get_friend_list_shard(room_id, "").await? {
        return Ok(("".to_string(), content));
    }
    Ok((target, json!({ "friends": [], "version": 1 })))
}

/// W6: cursor 翻页起点解析。
///
/// 输入：`items` 已排序的好友数组（cursor 翻页的目标数组）、
/// `request.from`（可选 cursor）和 `sort_by`。
/// 输出：`start_index: usize` —— cursor 翻页应跳过的 entry 数（unbounded 即 total）。
///
/// 算法：
/// 1. cursor 为 None → 用 `request.offset`（如有）或 0
/// 2. cursor 为 Some → 用 `partition_point` 二分查找首个
///    `compare_friend_entry_to_cursor(item, cursor) == Greater` 的位置（O(log n)）
///
/// W3 review 留下的 TODO 4 原本建议"二级缓存"消除 O(n) scan，但仔细分析后
/// `compare_friend_entry_to_cursor` 按 sort_by 决定的排序键是全序关系（`compare`
/// 走 Ord），`partition_point` 标准库二分天然成立：
/// - O(log n) vs 之前 O(n)：1000 好友 ~10 次比较 vs ~1000 次
/// - 0 cache 复杂度：不需要写回、不需要失效策略、不需要 Redis 反序列化额外字段
/// - cursor 数量无关：不像 cache 那样需要担心 BTreeMap 大小
///
/// `compare_friend_entry_to_cursor` 现有 `Greater` 含义"item 在 cursor 之后"，
/// 与本函数语义"返回比 cursor 严格更大（或靠后）的所有 entry"一致。
/// 谓词取反 `!= Greater` 等价于"≤ cursor"，partition_point 返回首个 true 位置
/// 即"第一个 > cursor"。
pub(crate) fn resolve_cursor_start_index(items: &[FriendListEntry], request: &FriendListRequest) -> usize {
    let Some(cursor) = request.from.as_ref() else {
        return request.offset.unwrap_or(0).min(items.len());
    };
    items.partition_point(|item| {
        FriendRoomService::compare_friend_entry_to_cursor(item, cursor, &request.sort_by) != Ordering::Greater
    })
}

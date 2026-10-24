//! Shared response-assembly helpers used by both `/sync` and sliding sync.
//!
//! Event-to-JSON conversion is identical across both code paths (Client format).
//! Federation-format variants live in `sync_service::response` where the extra
//! depth/origin fields are needed.

use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;
use synapse_storage::event::RoomEvent;
use synapse_storage::StateEvent;

/// Convert a [`RoomEvent`] to its Client-format JSON representation.
pub fn room_event_to_json(event: &RoomEvent) -> Value {
    let now = current_timestamp_millis();
    // `saturating_sub` only clamps at the i64 type bounds, so a future
    // `origin_server_ts` (clock skew) would yield a negative age. Matrix's
    // `unsigned.age` represents elapsed time and must be non-negative, so
    // clamp at 0 explicitly.
    let age = now.saturating_sub(event.origin_server_ts).max(0);
    let mut obj = json!({
        "type": event.event_type,
        "content": event.content,
        "sender": event.user_id,
        "origin_server_ts": event.origin_server_ts,
        "event_id": event.event_id,
        "room_id": event.room_id,
        "unsigned": merged_unsigned(event.unsigned.as_ref(), age)
    });
    if let Some(ref state_key) = event.state_key {
        obj["state_key"] = json!(state_key);
    }
    obj
}

/// Convert a [`StateEvent`] to its Client-format JSON representation.
pub fn state_event_to_json(event: &StateEvent) -> Value {
    let now = current_timestamp_millis();
    let age = now.saturating_sub(event.origin_server_ts).max(0);
    let sender = event.user_id.as_deref().unwrap_or(&event.sender);
    let event_type = event.event_type.as_deref().unwrap_or("m.room.message");
    let mut obj = json!({
        "type": event_type,
        "content": event.content,
        "sender": sender,
        "origin_server_ts": event.origin_server_ts,
        "event_id": event.event_id,
        "room_id": event.room_id,
        "unsigned": merged_unsigned(event.unsigned.as_ref(), age)
    });
    if let Some(ref state_key) = event.state_key {
        obj["state_key"] = json!(state_key);
    }
    obj
}

/// Build the client-visible `unsigned` block: the server-computed `age` plus
/// whatever the stored JSONB `unsigned` carried (e.g. the
/// `msc4354_sticky_duration_ttl_ms` sticky TTL written by the `/send` hint).
///
/// `age` is authoritative and always wins over a stale stored value.
pub(crate) fn merged_unsigned(stored: Option<&Value>, age: i64) -> Value {
    let mut map = match stored {
        Some(Value::Object(map)) => map.clone(),
        _ => serde_json::Map::new(),
    };
    map.insert("age".to_string(), json!(age));
    Value::Object(map)
}

/// 存储层给出的时间线顺序。
///
/// 这是 `/sync` 与滑动同步在时间线上**唯一的**真实差异，必须显式化：以前的写法是
/// 两个服务各自 `truncate` + 各自 `reverse`，谁少写一句就漂移。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineOrder {
    /// 按 `stream_ordering` **降序**给出（批量查询、分页查询 `direction=b`）。
    NewestFirst,
    /// 已按 `stream_ordering` **升序**给出（`get_room_events_after_stream_ordering`）。
    OldestFirst,
}

/// 一条协议时间线窗口。
pub struct TimelineWindow {
    /// 事件，**升序**（`stream_ordering` 单调不减），最多 `limit` 条。
    pub events: Vec<RoomEvent>,
    /// 是否因为上限而丢弃了事件（客户端据此决定回翻）。
    pub limited: bool,
}

/// 时间线窗口的**唯一**实现：保留**最新** `limit` 条，升序输出，并给出 `limited`。
///
/// 为什么要有它（ARCH-02）：「取哪 N 条、什么顺序、是否 limited」这三个语义原先在
/// `/sync` 与滑动同步里各写了一遍，结果漂移成 `/sync` 拿**最旧** N 条、滑动同步拿
/// **最新** N 条（由 `tests/integration/sync_path_equivalence_tests.rs` 抓到）。现在
/// 只有这一份实现，两条路径都从这里取。
///
/// `limit == 0` 表示「不返回任何事件」，此时 `limited` 反映「原本是否有事件」——
/// 这是 `/sync` 的既有语义（空窗口 + `limited=true` 让客户端回翻）。
pub fn timeline_window(mut events: Vec<RoomEvent>, limit: usize, incoming: TimelineOrder) -> TimelineWindow {
    let limited = events.len() > limit;
    if limited {
        match incoming {
            // 最新在前 ⇒ 保留前 `limit` 条（即最新的 N 条），再翻成升序。
            TimelineOrder::NewestFirst => {
                events.truncate(limit);
                events.reverse();
            }
            // 已经升序 ⇒ 丢掉最旧的头部，保留尾部 `limit` 条。
            TimelineOrder::OldestFirst => {
                events.drain(..events.len() - limit);
            }
        }
    } else if incoming == TimelineOrder::NewestFirst {
        events.reverse();
    }
    TimelineWindow { events, limited }
}

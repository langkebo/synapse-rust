use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{de::Deserializer, Deserialize, Serialize};
use std::collections::HashMap;

/// The `SlidingSyncToken` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SlidingSyncToken {
    /// The `id` field.
    pub id: i64,
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: String,
    /// The `conn_id` field.
    pub conn_id: Option<String>,
    /// The `token` field.
    pub token: String,
    /// The `pos` field.
    pub pos: i64,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `expires_at` field.
    pub expires_at: Option<i64>,
    /// S14: 上一次同步开始时 events.stream_ordering 的快照，
    /// 作为本次增量同步 timeline 的起始水位线。
    pub event_stream_pos: i64,
}

/// The `SlidingSyncList` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SlidingSyncList {
    /// The `id` field.
    pub id: i64,
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: String,
    /// The `conn_id` field.
    pub conn_id: Option<String>,
    /// The `list_key` field.
    pub list_key: String,
    /// The `sort` field.
    pub sort: serde_json::Value,
    /// The `filters` field.
    pub filters: Option<serde_json::Value>,
    /// The `room_subscription` field.
    pub room_subscription: Option<serde_json::Value>,
    /// The `ranges` field.
    pub ranges: Option<serde_json::Value>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// The `SlidingSyncRoom` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SlidingSyncRoom {
    /// The `id` field.
    pub id: i64,
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `conn_id` field.
    pub conn_id: Option<String>,
    /// The `list_key` field.
    pub list_key: Option<String>,
    /// The `bump_stamp` field.
    pub bump_stamp: Option<i64>,
    /// The `highlight_count` field.
    pub highlight_count: i32,
    /// The `notification_count` field.
    pub notification_count: i32,
    /// The `is_dm` field.
    pub is_dm: bool,
    /// The `is_encrypted` field.
    pub is_encrypted: bool,
    /// The `is_tombstoned` field.
    pub is_tombstoned: bool,
    /// The `is_invited` field.
    pub is_invited: bool,
    /// The `name` field.
    pub name: Option<String>,
    /// The `avatar` field.
    pub avatar: Option<String>,
    /// The `timestamp` field.
    pub timestamp: Option<i64>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: i64,
}

/// The `SlidingSyncListQuery` struct.
pub struct SlidingSyncListQuery<'a> {
    /// The `user_id` field.
    pub user_id: &'a str,
    /// The `device_id` field.
    pub device_id: &'a str,
    /// The `conn_id` field.
    pub conn_id: Option<&'a str>,
    /// The `list_key` field.
    pub list_key: &'a str,
    /// The `start` field.
    pub start: u32,
    /// The `end` field.
    pub end: u32,
    /// The `filters` field.
    pub filters: Option<&'a SlidingSyncFilters>,
}

/// The `AdminRoomTokenSyncEntry` struct.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AdminRoomTokenSyncEntry {
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: String,
    /// The `conn_id` field.
    pub conn_id: Option<String>,
    /// The `list_key` field.
    pub list_key: Option<String>,
    /// The `pos` field.
    pub pos: Option<i64>,
    /// The `token_created_ts` field.
    pub token_created_ts: Option<i64>,
    /// The `token_expires_at` field.
    pub token_expires_at: Option<i64>,
    /// The `room_timestamp` field.
    pub room_timestamp: Option<i64>,
    /// The `room_updated_ts` field.
    pub room_updated_ts: i64,
    /// The `bump_stamp` field.
    pub bump_stamp: Option<i64>,
    /// The `highlight_count` field.
    pub highlight_count: i32,
    /// The `notification_count` field.
    pub notification_count: i32,
    /// The `is_dm` field.
    pub is_dm: bool,
    /// The `is_encrypted` field.
    pub is_encrypted: bool,
    /// The `is_tombstoned` field.
    pub is_tombstoned: bool,
    /// The `is_invited` field.
    pub is_invited: bool,
    /// The `name` field.
    pub name: Option<String>,
    /// The `avatar` field.
    pub avatar: Option<String>,
    /// The `is_expired` field.
    pub is_expired: bool,
}

/// The `RoomTokenSyncCursor` struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomTokenSyncCursor {
    /// The `room_updated_ts` field.
    pub room_updated_ts: i64,
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: String,
    /// The `conn_id` field.
    pub conn_id: Option<String>,
}

/// See [`encode_room_token_sync_cursor`].
pub fn encode_room_token_sync_cursor(cursor: &RoomTokenSyncCursor) -> String {
    let encoded_user_id = URL_SAFE_NO_PAD.encode(cursor.user_id.as_bytes());
    let encoded_device_id = URL_SAFE_NO_PAD.encode(cursor.device_id.as_bytes());
    let is_conn_id_null = if cursor.conn_id.is_none() { 1 } else { 0 };
    let encoded_conn_id = URL_SAFE_NO_PAD.encode(cursor.conn_id.as_deref().unwrap_or("").as_bytes());

    format!(
        "{}|{}|{}|{}|{}",
        cursor.room_updated_ts, encoded_user_id, encoded_device_id, is_conn_id_null, encoded_conn_id
    )
}

/// See [`decode_room_token_sync_cursor`].
pub fn decode_room_token_sync_cursor(cursor: Option<&str>) -> Option<RoomTokenSyncCursor> {
    let cursor = cursor?;
    let mut parts = cursor.split('|');
    let room_updated_ts = parts.next()?.parse::<i64>().ok()?;
    let encoded_user_id = parts.next()?;
    let encoded_device_id = parts.next()?;
    let is_conn_id_null = parts.next()?.parse::<u8>().ok()?;
    let encoded_conn_id = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    let user_id = String::from_utf8(URL_SAFE_NO_PAD.decode(encoded_user_id).ok()?).ok()?;
    let device_id = String::from_utf8(URL_SAFE_NO_PAD.decode(encoded_device_id).ok()?).ok()?;
    let conn_id = if is_conn_id_null == 1 {
        None
    } else {
        Some(String::from_utf8(URL_SAFE_NO_PAD.decode(encoded_conn_id).ok()?).ok()?)
    };

    if user_id.is_empty() || device_id.is_empty() {
        return None;
    }

    Some(RoomTokenSyncCursor { room_updated_ts, user_id, device_id, conn_id })
}

/// The `SlidingSyncRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlidingSyncRequest {
    /// The `conn_id` field.
    pub conn_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_sliding_sync_lists")]
    /// The `lists` field.
    pub lists: HashMap<String, SlidingSyncListData>,
    /// The `room_subscriptions` field.
    pub room_subscriptions: Option<serde_json::Value>,
    #[serde(default)]
    /// The `unsubscribe_rooms` field.
    pub unsubscribe_rooms: Option<Vec<String>>,
    /// The `extensions` field.
    pub extensions: Option<serde_json::Value>,
    /// The `pos` field.
    pub pos: Option<String>,
    /// The `timeout` field.
    pub timeout: Option<u32>,
    #[serde(rename = "clientTimeout")]
    /// The `client_timeout` field.
    pub client_timeout: Option<u32>,
    /// MSC4186: transaction ID for request idempotency. When provided, the
    /// server caches the response for this `txn_id` and returns the cached
    /// response if the same `txn_id` is seen again (e.g., due to retries).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// The `txn_id` field.
    pub txn_id: Option<String>,
}

fn deserialize_sliding_sync_lists<'de, D>(deserializer: D) -> Result<HashMap<String, SlidingSyncListData>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ListsPayload {
        Map(HashMap<String, SlidingSyncListData>),
        Vec(Vec<SlidingSyncListRequest>),
    }

    match ListsPayload::deserialize(deserializer)? {
        ListsPayload::Map(map) => Ok(map),
        ListsPayload::Vec(list_requests) => {
            let mut map = HashMap::new();
            for list in list_requests {
                let ranges = list.ranges.into_iter().map(|(start, end)| vec![start, end]).collect();
                map.insert(
                    list.list_key,
                    SlidingSyncListData {
                        ranges,
                        sort: list.sort,
                        filters: list.filters,
                        timeline_limit: list.limit,
                        required_state: None,
                        slow_by: None,
                        bump_event_types: None,
                    },
                );
            }
            Ok(map)
        }
    }
}

/// The `SlidingSyncListData` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlidingSyncListData {
    #[serde(default)]
    /// The `ranges` field.
    pub ranges: Vec<Vec<u32>>,
    #[serde(default)]
    /// The `sort` field.
    pub sort: Vec<String>,
    /// The `filters` field.
    pub filters: Option<SlidingSyncFilters>,
    #[serde(rename = "timeline_limit", alias = "timelineLimit", default)]
    /// The `timeline_limit` field.
    pub timeline_limit: Option<u32>,
    #[serde(rename = "required_state", alias = "requiredState", default)]
    /// The `required_state` field.
    pub required_state: Option<Vec<Vec<String>>>,
    #[serde(default)]
    /// The `slow_by` field.
    pub slow_by: Option<u32>,
    #[serde(default)]
    /// The `bump_event_types` field.
    pub bump_event_types: Option<Vec<String>>,
}

/// The `SlidingSyncListRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlidingSyncListRequest {
    /// The `list_key` field.
    pub list_key: String,
    /// The `sort` field.
    pub sort: Vec<String>,
    /// The `filters` field.
    pub filters: Option<SlidingSyncFilters>,
    /// The `room_subscription` field.
    pub room_subscription: Option<serde_json::Value>,
    /// The `ranges` field.
    pub ranges: Vec<(u32, u32)>,
    /// The `limit` field.
    pub limit: Option<u32>,
}

/// The `SlidingSyncFilters` struct.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SlidingSyncFilters {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    /// The `is_dm` field.
    pub is_dm: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    /// The `is_encrypted` field.
    pub is_encrypted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    /// The `is_invite` field.
    pub is_invite: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    /// The `is_tombstoned` field.
    pub is_tombstoned: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    /// The `room_types` field.
    pub room_types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    /// The `not_room_types` field.
    pub not_room_types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    /// The `room_name_like` field.
    pub room_name_like: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    /// The `tags` field.
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    /// The `not_tags` field.
    pub not_tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    /// The `room_state_types` field.
    pub room_state_types: Option<Vec<String>>,
}

/// The `SlidingSyncResponse` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlidingSyncResponse {
    /// The `pos` field.
    pub pos: String,
    /// The `conn_id` field.
    pub conn_id: Option<String>,
    /// The `lists` field.
    pub lists: serde_json::Value,
    /// The `rooms` field.
    pub rooms: serde_json::Value,
    /// The `extensions` field.
    pub extensions: Option<serde_json::Value>,
}

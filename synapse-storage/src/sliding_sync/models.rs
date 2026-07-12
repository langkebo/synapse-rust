use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{de::Deserializer, Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SlidingSyncToken {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub conn_id: Option<String>,
    pub token: String,
    pub pos: i64,
    pub created_ts: i64,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SlidingSyncList {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub conn_id: Option<String>,
    pub list_key: String,
    pub sort: serde_json::Value,
    pub filters: Option<serde_json::Value>,
    pub room_subscription: Option<serde_json::Value>,
    pub ranges: Option<serde_json::Value>,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SlidingSyncRoom {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub room_id: String,
    pub conn_id: Option<String>,
    pub list_key: Option<String>,
    pub bump_stamp: Option<i64>,
    pub highlight_count: i32,
    pub notification_count: i32,
    pub is_dm: bool,
    pub is_encrypted: bool,
    pub is_tombstoned: bool,
    #[serde(rename = "invited")]
    #[sqlx(rename = "invited")]
    pub is_invited: bool,
    pub name: Option<String>,
    pub avatar: Option<String>,
    pub timestamp: Option<i64>,
    pub created_ts: i64,
    pub updated_ts: i64,
}

pub struct SlidingSyncListQuery<'a> {
    pub user_id: &'a str,
    pub device_id: &'a str,
    pub conn_id: Option<&'a str>,
    pub list_key: &'a str,
    pub start: u32,
    pub end: u32,
    pub filters: Option<&'a SlidingSyncFilters>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AdminRoomTokenSyncEntry {
    pub user_id: String,
    pub device_id: String,
    pub conn_id: Option<String>,
    pub list_key: Option<String>,
    pub pos: Option<i64>,
    pub token_created_ts: Option<i64>,
    pub token_expires_at: Option<i64>,
    pub room_timestamp: Option<i64>,
    pub room_updated_ts: i64,
    pub bump_stamp: Option<i64>,
    pub highlight_count: i32,
    pub notification_count: i32,
    pub is_dm: bool,
    pub is_encrypted: bool,
    pub is_tombstoned: bool,
    #[serde(rename = "invited")]
    #[sqlx(rename = "invited")]
    pub is_invited: bool,
    pub name: Option<String>,
    pub avatar: Option<String>,
    pub is_expired: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomTokenSyncCursor {
    pub room_updated_ts: i64,
    pub user_id: String,
    pub device_id: String,
    pub conn_id: Option<String>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlidingSyncRequest {
    pub conn_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_sliding_sync_lists")]
    pub lists: HashMap<String, SlidingSyncListData>,
    pub room_subscriptions: Option<serde_json::Value>,
    #[serde(default)]
    pub unsubscribe_rooms: Option<Vec<String>>,
    pub extensions: Option<serde_json::Value>,
    pub pos: Option<String>,
    pub timeout: Option<u32>,
    #[serde(rename = "clientTimeout")]
    pub client_timeout: Option<u32>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlidingSyncListData {
    #[serde(default)]
    pub ranges: Vec<Vec<u32>>,
    #[serde(default)]
    pub sort: Vec<String>,
    pub filters: Option<SlidingSyncFilters>,
    #[serde(rename = "timeline_limit", alias = "timelineLimit", default)]
    pub timeline_limit: Option<u32>,
    #[serde(rename = "required_state", alias = "requiredState", default)]
    pub required_state: Option<Vec<Vec<String>>>,
    #[serde(default)]
    pub slow_by: Option<u32>,
    #[serde(default)]
    pub bump_event_types: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlidingSyncListRequest {
    pub list_key: String,
    pub sort: Vec<String>,
    pub filters: Option<SlidingSyncFilters>,
    pub room_subscription: Option<serde_json::Value>,
    pub ranges: Vec<(u32, u32)>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SlidingSyncFilters {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub is_dm: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub is_encrypted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub is_invite: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub is_tombstoned: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub room_types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub not_room_types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub room_name_like: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub not_tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub room_state_types: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlidingSyncResponse {
    pub pos: String,
    pub conn_id: Option<String>,
    pub lists: serde_json::Value,
    pub rooms: serde_json::Value,
    pub extensions: Option<serde_json::Value>,
}

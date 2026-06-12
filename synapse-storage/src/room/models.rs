use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomSearchOrder {
    Created,
    Name,
    Size,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomSearchCursor {
    Created { created_ts: i64, room_id: String },
    Name { name: Option<String>, created_ts: i64, room_id: String },
    Size { member_count: i64, created_ts: i64, room_id: String },
}

impl RoomSearchOrder {
    pub fn from_query(order_by: Option<&str>) -> Self {
        match order_by {
            Some("name") => Self::Name,
            Some("size") => Self::Size,
            Some("created") | None => Self::Created,
            Some(_) => Self::Created,
        }
    }
}

pub fn encode_room_search_cursor(cursor: &RoomSearchCursor) -> String {
    match cursor {
        RoomSearchCursor::Created { created_ts, room_id } => format!("created|{created_ts}|{room_id}"),
        RoomSearchCursor::Name { name, created_ts, room_id } => {
            let is_null = if name.is_none() { 1 } else { 0 };
            let encoded_name = URL_SAFE_NO_PAD.encode(name.as_deref().unwrap_or(""));
            format!("name|{is_null}|{encoded_name}|{created_ts}|{room_id}")
        }
        RoomSearchCursor::Size { member_count, created_ts, room_id } => {
            format!("size|{member_count}|{created_ts}|{room_id}")
        }
    }
}

pub fn decode_room_search_cursor(cursor: Option<&str>) -> Option<RoomSearchCursor> {
    let cursor = cursor?;
    let mut parts = cursor.split('|');
    match parts.next()? {
        "created" => {
            let created_ts = parts.next()?.parse::<i64>().ok()?;
            let room_id = parts.next()?.to_string();
            if room_id.is_empty() || parts.next().is_some() {
                return None;
            }
            Some(RoomSearchCursor::Created { created_ts, room_id })
        }
        "name" => {
            let is_null = parts.next()?.parse::<u8>().ok()?;
            let encoded_name = parts.next()?;
            let created_ts = parts.next()?.parse::<i64>().ok()?;
            let room_id = parts.next()?.to_string();
            if room_id.is_empty() || parts.next().is_some() {
                return None;
            }
            let decoded_name = URL_SAFE_NO_PAD.decode(encoded_name).ok()?;
            let decoded_name = String::from_utf8(decoded_name).ok()?;
            Some(RoomSearchCursor::Name {
                name: if is_null == 1 { None } else { Some(decoded_name) },
                created_ts,
                room_id,
            })
        }
        "size" => {
            let member_count = parts.next()?.parse::<i64>().ok()?;
            let created_ts = parts.next()?.parse::<i64>().ok()?;
            let room_id = parts.next()?.to_string();
            if room_id.is_empty() || parts.next().is_some() {
                return None;
            }
            Some(RoomSearchCursor::Size { member_count, created_ts, room_id })
        }
        _ => None,
    }
}

#[cfg(test)]
mod cursor_tests {
    use super::{decode_room_search_cursor, encode_room_search_cursor, RoomSearchCursor};

    #[test]
    fn test_room_search_created_cursor_round_trip() {
        let cursor = encode_room_search_cursor(&RoomSearchCursor::Created {
            created_ts: 1_700_000_000_000,
            room_id: "!room:example.com".to_string(),
        });
        assert_eq!(
            decode_room_search_cursor(Some(&cursor)),
            Some(RoomSearchCursor::Created { created_ts: 1_700_000_000_000, room_id: "!room:example.com".to_string() })
        );
    }

    #[test]
    fn test_room_search_name_cursor_round_trip() {
        let cursor = encode_room_search_cursor(&RoomSearchCursor::Name {
            name: Some("Alpha|Beta".to_string()),
            created_ts: 1_700_000_000_000,
            room_id: "!room:example.com".to_string(),
        });
        assert_eq!(
            decode_room_search_cursor(Some(&cursor)),
            Some(RoomSearchCursor::Name {
                name: Some("Alpha|Beta".to_string()),
                created_ts: 1_700_000_000_000,
                room_id: "!room:example.com".to_string(),
            })
        );
    }

    #[test]
    fn test_room_search_size_cursor_round_trip() {
        let cursor = encode_room_search_cursor(&RoomSearchCursor::Size {
            member_count: 42,
            created_ts: 1_700_000_000_000,
            room_id: "!room:example.com".to_string(),
        });
        assert_eq!(
            decode_room_search_cursor(Some(&cursor)),
            Some(RoomSearchCursor::Size {
                member_count: 42,
                created_ts: 1_700_000_000_000,
                room_id: "!room:example.com".to_string(),
            })
        );
    }

    #[test]
    fn test_room_search_cursor_rejects_invalid_value() {
        assert_eq!(decode_room_search_cursor(Some("bad-cursor")), None);
        assert_eq!(decode_room_search_cursor(Some("created|123|")), None);
        assert_eq!(decode_room_search_cursor(Some("name|0|bad%%%|123|!room:example.com")), None);
    }
}

pub const DEFAULT_JOIN_RULE: &str = "invite";
pub const DEFAULT_HISTORY_VISIBILITY: &str = "joined";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub canonical_alias: Option<String>,
    pub join_rule: String,
    pub creator_user_id: Option<String>,
    pub room_version: String,
    pub encryption: Option<String>,
    pub is_public: bool,
    pub member_count: i64,
    pub history_visibility: String,
    pub created_ts: i64,
    pub is_federatable: bool,
    pub is_spotlight: bool,
    pub is_flagged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomEncryptionStatus {
    pub is_encrypted: bool,
    pub algorithm: Option<String>,
    pub rotation_period_ms: Option<i64>,
    pub rotation_period_msgs: Option<i64>,
}

impl RoomEncryptionStatus {
    pub fn from_room(room: &Room) -> Self {
        Self {
            is_encrypted: room.encryption.is_some(),
            algorithm: room.encryption.clone(),
            rotation_period_ms: None,
            rotation_period_msgs: None,
        }
    }

    pub fn from_encryption_event(
        is_encrypted: bool,
        algorithm: Option<String>,
        rotation_period_ms: Option<i64>,
        rotation_period_msgs: Option<i64>,
    ) -> Self {
        Self { is_encrypted, algorithm, rotation_period_ms, rotation_period_msgs }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub user_id: String,
    pub event_id: String,
    pub receipt_type: String,
    pub ts: i64,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct RoomRecord {
    pub(crate) room_id: String,
    pub(crate) name: Option<String>,
    pub(crate) topic: Option<String>,
    pub(crate) avatar_url: Option<String>,
    pub(crate) canonical_alias: Option<String>,
    #[sqlx(rename = "join_rules")]
    pub(crate) join_rule: Option<String>,
    pub(crate) creator: Option<String>,
    pub(crate) room_version: Option<String>,
    pub(crate) is_public: Option<bool>,
    pub(crate) member_count: Option<i64>,
    pub(crate) is_encrypted: Option<bool>,
    pub(crate) history_visibility: Option<String>,
    pub(crate) created_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct RoomWithMembersRecord {
    pub(crate) room_id: String,
    pub(crate) name: Option<String>,
    pub(crate) topic: Option<String>,
    pub(crate) avatar_url: Option<String>,
    pub(crate) canonical_alias: Option<String>,
    #[sqlx(rename = "join_rules")]
    pub(crate) join_rule: Option<String>,
    pub(crate) creator: Option<String>,
    pub(crate) room_version: Option<String>,
    pub(crate) is_public: Option<bool>,
    pub(crate) member_count: Option<i64>,
    pub(crate) is_encrypted: Option<bool>,
    pub(crate) history_visibility: Option<String>,
    pub(crate) created_ts: i64,
    pub(crate) joined_members: Option<i64>,
}

#[derive(Clone)]
pub struct RoomStorage {
    pub pool: Arc<Pool<Postgres>>,
}

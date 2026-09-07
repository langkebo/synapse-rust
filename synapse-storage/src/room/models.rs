use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;

use synapse_common::room_versions::DEFAULT_ROOM_VERSION;

/// The `RoomSearchOrder` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomSearchOrder {
    /// The `Created` variant.
    Created,
    /// The `Name` variant.
    Name,
    /// The `Size` variant.
    Size,
}

/// The `RoomSearchCursor` enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomSearchCursor {
    /// The `Created` variant.
    Created {
        /// The `created_ts` field.
        created_ts: i64,
        /// The `room_id` field.
        room_id: String,
    },
    /// The `Name` variant.
    Name {
        /// The `name` field.
        name: Option<String>,
        /// The `created_ts` field.
        created_ts: i64,
        /// The `room_id` field.
        room_id: String,
    },
    /// The `Size` variant.
    Size {
        /// The `member_count` field.
        member_count: i64,
        /// The `created_ts` field.
        created_ts: i64,
        /// The `room_id` field.
        room_id: String,
    },
}

impl RoomSearchOrder {
    /// See [`from_query`].
    /// See [`from_query`].
    pub fn from_query(order_by: Option<&str>) -> Self {
        match order_by {
            Some("name") => Self::Name,
            Some("size") => Self::Size,
            Some("created") | None => Self::Created,
            Some(_) => Self::Created,
        }
    }
}

/// See [`encode_room_search_cursor`].
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

/// See [`decode_room_search_cursor`].
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

/// Constant `DEFAULT_JOIN_RULE`.
pub const DEFAULT_JOIN_RULE: &str = "invite";
/// Constant `DEFAULT_HISTORY_VISIBILITY`.
pub const DEFAULT_HISTORY_VISIBILITY: &str = "joined";

/// The `Room` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    /// The `room_id` field.
    pub room_id: String,
    /// The `name` field.
    pub name: Option<String>,
    /// The `topic` field.
    pub topic: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `canonical_alias` field.
    pub canonical_alias: Option<String>,
    /// The `join_rule` field.
    pub join_rule: String,
    /// The `creator_user_id` field.
    pub creator_user_id: Option<String>,
    /// The `room_version` field.
    pub room_version: String,
    /// The `encryption` field.
    pub encryption: Option<String>,
    /// The `is_public` field.
    pub is_public: bool,
    /// The `member_count` field.
    pub member_count: i64,
    /// The `history_visibility` field.
    pub history_visibility: String,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `is_federatable` field.
    pub is_federatable: bool,
    /// The `is_spotlight` field.
    pub is_spotlight: bool,
    /// The `is_flagged` field.
    pub is_flagged: bool,
}

/// The `RoomEncryptionStatus` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomEncryptionStatus {
    /// The `is_encrypted` field.
    pub is_encrypted: bool,
    /// The `algorithm` field.
    pub algorithm: Option<String>,
    /// The `rotation_period_ms` field.
    pub rotation_period_ms: Option<i64>,
    /// The `rotation_period_msgs` field.
    pub rotation_period_msgs: Option<i64>,
}

impl RoomEncryptionStatus {
    /// See [`from_room`].
    /// See [`from_room`].
    pub fn from_room(room: &Room) -> Self {
        Self {
            is_encrypted: room.encryption.is_some(),
            algorithm: room.encryption.clone(),
            rotation_period_ms: None,
            rotation_period_msgs: None,
        }
    }

    /// See [`from_encryption_event`].
    pub fn from_encryption_event(
        is_encrypted: bool,
        algorithm: Option<String>,
        rotation_period_ms: Option<i64>,
        rotation_period_msgs: Option<i64>,
    ) -> Self {
        Self { is_encrypted, algorithm, rotation_period_ms, rotation_period_msgs }
    }
}

/// The `Receipt` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    /// The `user_id` field.
    pub user_id: String,
    /// The `event_id` field.
    pub event_id: String,
    /// The `receipt_type` field.
    pub receipt_type: String,
    /// The `ts` field.
    pub ts: i64,
    /// The `data` field.
    pub data: serde_json::Value,
}

/// The `RoomUnreadCounts` struct.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct RoomUnreadCounts {
    /// The `room_id` field.
    pub room_id: String,
    /// The `highlight_count` field.
    pub highlight_count: i64,
    /// The `notification_count` field.
    pub notification_count: i64,
}

/// The `RoomRecord` struct.
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct RoomRecord {
    pub(crate) room_id: String,
    pub(crate) name: Option<String>,
    pub(crate) topic: Option<String>,
    pub(crate) avatar_url: Option<String>,
    pub(crate) canonical_alias: Option<String>,
    #[sqlx(rename = "join_rules")]
    pub(crate) join_rule: Option<String>,
    #[sqlx(rename = "creator")]
    pub(crate) creator_user_id: Option<String>,
    pub(crate) room_version: Option<String>,
    pub(crate) is_public: Option<bool>,
    pub(crate) member_count: Option<i64>,
    pub(crate) is_encrypted: Option<bool>,
    pub(crate) history_visibility: Option<String>,
    pub(crate) created_ts: i64,
}

/// The `RoomWithMembersRecord` struct.
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct RoomWithMembersRecord {
    pub(crate) room_id: String,
    pub(crate) name: Option<String>,
    pub(crate) topic: Option<String>,
    pub(crate) avatar_url: Option<String>,
    pub(crate) canonical_alias: Option<String>,
    #[sqlx(rename = "join_rules")]
    pub(crate) join_rule: Option<String>,
    #[sqlx(rename = "creator")]
    pub(crate) creator_user_id: Option<String>,
    pub(crate) room_version: Option<String>,
    pub(crate) is_public: Option<bool>,
    pub(crate) member_count: Option<i64>,
    pub(crate) is_encrypted: Option<bool>,
    pub(crate) history_visibility: Option<String>,
    pub(crate) created_ts: i64,
    pub(crate) joined_members: Option<i64>,
}

/// The `RoomStorage` struct.
#[derive(Clone)]
pub struct RoomStorage {
    /// The `pool` field.
    pub pool: Arc<Pool<Postgres>>,
}

impl RoomStorage {
    /// Search the room directory (public rooms) by name/topic.
    ///
    /// This inherent method was added to support the `RoomRepository` trait;
    /// it did not previously exist on `RoomStorage`.
    pub async fn search_room_directory(&self, search_term: &str, limit: i64) -> Result<Vec<Room>, sqlx::Error> {
        let pattern = format!("%{}%", search_term.to_lowercase());
        let rows: Vec<RoomRecord> = sqlx::query_as(
            r"
            SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias, r.join_rules, r.creator,
                   r.room_version, r.is_public, rs.member_count as member_count,
                   rs.is_encrypted as is_encrypted, r.history_visibility, r.created_ts
            FROM rooms r
            LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
            WHERE r.is_public = TRUE
              AND (LOWER(r.name) LIKE $1 OR LOWER(r.topic) LIKE $1)
            ORDER BY r.name
            LIMIT $2
            ",
        )
        .bind(&pattern)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|row| Room {
                room_id: row.room_id.clone(),
                name: row.name.clone(),
                topic: row.topic.clone(),
                avatar_url: row.avatar_url.clone(),
                canonical_alias: row.canonical_alias.clone(),
                join_rule: row.join_rule.clone().unwrap_or_else(|| DEFAULT_JOIN_RULE.to_string()),
                creator_user_id: row.creator_user_id.clone(),
                room_version: row.room_version.clone().unwrap_or_else(|| DEFAULT_ROOM_VERSION.to_string()),
                encryption: Self::encryption_from_is_encrypted(row.is_encrypted),
                is_public: row.is_public.unwrap_or(false),
                member_count: row.member_count.unwrap_or(0),
                history_visibility: row
                    .history_visibility
                    .clone()
                    .unwrap_or_else(|| DEFAULT_HISTORY_VISIBILITY.to_string()),
                created_ts: row.created_ts,
                is_federatable: true,
                is_spotlight: false,
                is_flagged: false,
            })
            .collect())
    }
}

use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;

use super::repository::RoomRepository;
use synapse_common::room_versions::DEFAULT_ROOM_VERSION;

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

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct RoomUnreadCounts {
    pub room_id: String,
    pub highlight_count: i64,
    pub notification_count: i64,
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

impl RoomStorage {
    /// Search the room directory (public rooms) by name/topic.
    ///
    /// This inherent method was added to support the `RoomRepository` trait;
    /// it did not previously exist on `RoomStorage`.
    pub async fn search_room_directory(
        &self,
        search_term: &str,
        limit: i64,
    ) -> Result<Vec<Room>, sqlx::Error> {
        let pattern = format!("%{}%", search_term);
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
                creator_user_id: row.creator.clone(),
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

#[async_trait]
impl RoomRepository for RoomStorage {
    async fn get_room(&self, room_id: &str) -> Result<Option<Room>, sqlx::Error> {
        self.get_room(room_id).await
    }

    async fn get_rooms_batch(&self, room_ids: &[String]) -> Result<Vec<Room>, sqlx::Error> {
        self.get_rooms_batch(room_ids).await
    }

    async fn create_room(
        &self,
        room_id: &str,
        creator: &str,
        join_rule: &str,
        room_version: &str,
        is_public: bool,
    ) -> Result<Room, sqlx::Error> {
        self.create_room(room_id, creator, join_rule, room_version, is_public).await
    }

    async fn update_room_name(&self, room_id: &str, name: &str) -> Result<(), sqlx::Error> {
        self.update_room_name(room_id, name).await
    }

    async fn update_room_topic(&self, room_id: &str, topic: &str) -> Result<(), sqlx::Error> {
        self.update_room_topic(room_id, topic).await
    }

    async fn set_room_public(&self, room_id: &str, is_public: bool) -> Result<(), sqlx::Error> {
        // NOTE: Delegates to `set_room_directory` — the only inherent method
        // that updates both `rooms.is_public` and the `room_directory` table.
        self.set_room_directory(room_id, is_public).await
    }

    async fn delete_room(&self, room_id: &str) -> Result<(), sqlx::Error> {
        self.delete_room(room_id).await
    }

    async fn get_public_rooms(&self, limit: i64) -> Result<Vec<Room>, sqlx::Error> {
        self.get_public_rooms(limit).await
    }

    async fn get_user_rooms(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        self.get_user_rooms(user_id).await
    }

    async fn search_room_directory(
        &self,
        search_term: &str,
        limit: i64,
    ) -> Result<Vec<Room>, sqlx::Error> {
        self.search_room_directory(search_term, limit).await
    }
}

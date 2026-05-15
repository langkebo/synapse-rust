use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::json;
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
    Created {
        created_ts: i64,
        room_id: String,
    },
    Name {
        name: Option<String>,
        created_ts: i64,
        room_id: String,
    },
    Size {
        member_count: i64,
        created_ts: i64,
        room_id: String,
    },
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
        RoomSearchCursor::Created {
            created_ts,
            room_id,
        } => format!("created|{created_ts}|{room_id}"),
        RoomSearchCursor::Name {
            name,
            created_ts,
            room_id,
        } => {
            let is_null = if name.is_none() { 1 } else { 0 };
            let encoded_name = URL_SAFE_NO_PAD.encode(name.as_deref().unwrap_or(""));
            format!("name|{is_null}|{encoded_name}|{created_ts}|{room_id}")
        }
        RoomSearchCursor::Size {
            member_count,
            created_ts,
            room_id,
        } => format!("size|{member_count}|{created_ts}|{room_id}"),
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
            Some(RoomSearchCursor::Created {
                created_ts,
                room_id,
            })
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
                name: if is_null == 1 {
                    None
                } else {
                    Some(decoded_name)
                },
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
            Some(RoomSearchCursor::Size {
                member_count,
                created_ts,
                room_id,
            })
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
            Some(RoomSearchCursor::Created {
                created_ts: 1_700_000_000_000,
                room_id: "!room:example.com".to_string(),
            })
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
        assert_eq!(
            decode_room_search_cursor(Some("name|0|bad%%%|123|!room:example.com")),
            None
        );
    }
}

const DEFAULT_JOIN_RULE: &str = "invite";
const DEFAULT_HISTORY_VISIBILITY: &str = "joined";
const DEFAULT_ROOM_VERSION: &str = "10";

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
pub struct Receipt {
    pub user_id: String,
    pub event_id: String,
    pub receipt_type: String,
    pub ts: i64,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct RoomRecord {
    room_id: String,
    name: Option<String>,
    topic: Option<String>,
    avatar_url: Option<String>,
    canonical_alias: Option<String>,
    #[sqlx(rename = "join_rules")]
    join_rule: Option<String>,
    creator: Option<String>,
    room_version: Option<String>,
    is_public: Option<bool>,
    member_count: Option<i64>,
    is_encrypted: Option<bool>,
    history_visibility: Option<String>,
    created_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct RoomWithMembersRecord {
    room_id: String,
    name: Option<String>,
    topic: Option<String>,
    avatar_url: Option<String>,
    canonical_alias: Option<String>,
    #[sqlx(rename = "join_rules")]
    join_rule: Option<String>,
    creator: Option<String>,
    room_version: Option<String>,
    is_public: Option<bool>,
    member_count: Option<i64>,
    is_encrypted: Option<bool>,
    history_visibility: Option<String>,
    created_ts: i64,
    joined_members: Option<i64>,
}

#[derive(Clone)]
pub struct RoomStorage {
    pub pool: Arc<Pool<Postgres>>,
}

impl RoomStorage {
    fn encryption_from_is_encrypted(is_encrypted: Option<bool>) -> Option<String> {
        if is_encrypted.unwrap_or(false) {
            Some("m.megolm.v1.aes-sha2".to_string())
        } else {
            None
        }
    }

    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_room(
        &self,
        room_id: &str,
        creator: &str,
        join_rule: &str,
        version: &str,
        is_public: bool,
    ) -> Result<Room, sqlx::Error> {
        Self::create_room_with_executor(
            &*self.pool,
            room_id,
            creator,
            join_rule,
            version,
            is_public,
        )
        .await
    }

    pub async fn create_room_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        room_id: &str,
        creator: &str,
        join_rule: &str,
        version: &str,
        is_public: bool,
    ) -> Result<Room, sqlx::Error> {
        Self::create_room_with_executor(&mut **tx, room_id, creator, join_rule, version, is_public)
            .await
    }

    async fn create_room_with_executor<'a, E>(
        executor: E,
        room_id: &str,
        creator: &str,
        join_rule: &str,
        version: &str,
        is_public: bool,
    ) -> Result<Room, sqlx::Error>
    where
        E: sqlx::Executor<'a, Database = Postgres>,
    {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"
            INSERT INTO rooms (room_id, creator, join_rules, room_version, is_public, history_visibility, created_ts, last_activity_ts)
            VALUES ($1, $2, $3, $4, $5, 'joined', $6, $6)
            "#,
        )
        .bind(room_id)
        .bind(creator)
        .bind(join_rule)
        .bind(version)
        .bind(is_public)
        .bind(now)
        .execute(executor)
        .await?;

        Ok(Room {
            room_id: room_id.to_string(),
            name: None,
            topic: None,
            avatar_url: None,
            canonical_alias: None,
            join_rule: join_rule.to_string(),
            creator_user_id: Some(creator.to_string()),
            room_version: version.to_string(),
            encryption: None,
            is_public,
            member_count: 1,
            history_visibility: DEFAULT_HISTORY_VISIBILITY.to_string(),
            created_ts: now,
            is_federatable: true,
            is_spotlight: false,
            is_flagged: false,
        })
    }

    pub async fn get_room(&self, room_id: &str) -> Result<Option<Room>, sqlx::Error> {
        let row = sqlx::query_as::<_, RoomRecord>(
            r#"
            SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias, r.join_rules, r.creator, r.room_version,
                  COALESCE(rs.member_count, joined.joined_members, 0) as member_count, rs.is_encrypted as is_encrypted, r.is_public, r.history_visibility, r.created_ts
            FROM rooms r
            LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
            LEFT JOIN (
                SELECT room_id, COUNT(*)::BIGINT AS joined_members
                FROM room_memberships
                WHERE membership = 'join'
                GROUP BY room_id
            ) joined ON joined.room_id = r.room_id
            WHERE r.room_id = $1
            "#,
        )
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await?;
        if let Some(row) = row {
            Ok(Some(Room {
                room_id: row.room_id,
                name: row.name,
                topic: row.topic,
                avatar_url: row.avatar_url,
                canonical_alias: row.canonical_alias,
                join_rule: row
                    .join_rule
                    .unwrap_or_else(|| DEFAULT_JOIN_RULE.to_string()),
                creator_user_id: row.creator,
                room_version: row
                    .room_version
                    .unwrap_or_else(|| DEFAULT_ROOM_VERSION.to_string()),
                encryption: Self::encryption_from_is_encrypted(row.is_encrypted),
                is_public: row.is_public.unwrap_or(false),
                member_count: row.member_count.unwrap_or(0),
                history_visibility: row
                    .history_visibility
                    .unwrap_or_else(|| DEFAULT_HISTORY_VISIBILITY.to_string()),
                created_ts: row.created_ts,
                is_federatable: true,
                is_spotlight: false,
                is_flagged: false,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn get_rooms_batch(&self, room_ids: &[String]) -> Result<Vec<Room>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(Vec::new());
        }

        let rows: Vec<RoomRecord> = sqlx::query_as(
            r#"
            SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias, r.join_rules, r.creator, r.room_version,
                  r.is_public, rs.member_count as member_count, rs.is_encrypted as is_encrypted, r.history_visibility, r.created_ts
            FROM rooms r
            LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
            WHERE r.room_id = ANY($1)
            "#,
        )
        .bind(room_ids)
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
                join_rule: row
                    .join_rule
                    .clone()
                    .unwrap_or_else(|| DEFAULT_JOIN_RULE.to_string()),
                creator_user_id: row.creator.clone(),
                room_version: row
                    .room_version
                    .clone()
                    .unwrap_or_else(|| DEFAULT_ROOM_VERSION.to_string()),
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

    pub async fn get_room_creator(&self, room_id: &str) -> Result<Option<String>, sqlx::Error> {
        let result: Option<(String,)> = sqlx::query_as(
            r#"
            SELECT creator FROM rooms WHERE room_id = $1
            "#,
        )
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.map(|r| r.0))
    }

    pub async fn room_exists(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar::<_, i32>(
            r#"
            SELECT 1 AS "exists" FROM rooms WHERE room_id = $1 LIMIT 1
            "#,
        )
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.is_some())
    }

    pub async fn get_public_rooms(&self, limit: i64) -> Result<Vec<Room>, sqlx::Error> {
        self.get_public_rooms_paginated(limit, None, None).await
    }

    /// 支持分页的 public rooms 列表。使用 Keyset 分页 (created_ts, room_id)。
    pub async fn get_public_rooms_paginated(
        &self,
        limit: i64,
        since_ts: Option<i64>,
        since_room_id: Option<&str>,
    ) -> Result<Vec<Room>, sqlx::Error> {
        let rows: Vec<RoomRecord> = if let (Some(ts), Some(room_id)) = (since_ts, since_room_id) {
            sqlx::query_as(
                r#"
                SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias, r.join_rules, r.creator, r.room_version,
                      r.is_public, rs.member_count as member_count, rs.is_encrypted as is_encrypted, r.history_visibility, r.created_ts
                FROM rooms r
                LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
                WHERE r.is_public = TRUE AND (r.created_ts < $2 OR (r.created_ts = $2 AND r.room_id < $3))
                ORDER BY r.created_ts DESC, r.room_id DESC
                LIMIT $1
                "#,
            )
            .bind(limit)
            .bind(ts)
            .bind(room_id)
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query_as(
                r#"
                SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias, r.join_rules, r.creator, r.room_version,
                      r.is_public, rs.member_count as member_count, rs.is_encrypted as is_encrypted, r.history_visibility, r.created_ts
                FROM rooms r
                LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
                WHERE r.is_public = TRUE
                ORDER BY r.created_ts DESC, r.room_id DESC
                LIMIT $1
                "#,
            )
            .bind(limit)
            .fetch_all(&*self.pool)
            .await?
        };
        Ok(rows
            .iter()
            .map(|row| Room {
                room_id: row.room_id.clone(),
                name: row.name.clone(),
                topic: row.topic.clone(),
                avatar_url: row.avatar_url.clone(),
                canonical_alias: row.canonical_alias.clone(),
                join_rule: row
                    .join_rule
                    .clone()
                    .unwrap_or_else(|| DEFAULT_JOIN_RULE.to_string()),
                creator_user_id: row.creator.clone(),
                room_version: row
                    .room_version
                    .clone()
                    .unwrap_or_else(|| DEFAULT_ROOM_VERSION.to_string()),
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

    /// 返回公开房间总数，用于 `total_room_count_estimate` 字段。
    pub async fn count_public_rooms(&self) -> Result<i64, sqlx::Error> {
        let count: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM rooms WHERE is_public = TRUE
            "#,
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(count.0)
    }

    pub async fn get_all_rooms_with_members(
        &self,
        limit: i64,
        from: Option<RoomSearchCursor>,
        order_by: RoomSearchOrder,
    ) -> Result<(Vec<(Room, i64)>, Option<String>), sqlx::Error> {
        let mut query_builder: sqlx::QueryBuilder<Postgres> = sqlx::QueryBuilder::new(
            r#"
            SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias, r.join_rules, r.creator,
                r.room_version, r.is_public, rs.member_count as member_count, rs.is_encrypted as is_encrypted, r.history_visibility,
                r.created_ts, COUNT(rm.user_id) as joined_members
            FROM rooms r
            LEFT JOIN room_memberships rm ON r.room_id = rm.room_id AND rm.membership = 'join'
            LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
            "#,
        );

        query_builder.push(" WHERE 1 = 1 ");

        match (order_by, from) {
            (
                RoomSearchOrder::Created,
                Some(RoomSearchCursor::Created {
                    created_ts,
                    room_id,
                }),
            ) => {
                query_builder.push(" AND (r.created_ts, r.room_id) < (");
                query_builder.push_bind(created_ts);
                query_builder.push(", ");
                query_builder.push_bind(room_id);
                query_builder.push(")");
            }
            (
                RoomSearchOrder::Name,
                Some(RoomSearchCursor::Name {
                    name,
                    created_ts,
                    room_id,
                }),
            ) => {
                query_builder.push(" AND (r.name, r.created_ts, r.room_id) < (");
                query_builder.push_bind(name);
                query_builder.push(", ");
                query_builder.push_bind(created_ts);
                query_builder.push(", ");
                query_builder.push_bind(room_id);
                query_builder.push(")");
            }
            (
                RoomSearchOrder::Size,
                Some(RoomSearchCursor::Size {
                    member_count,
                    created_ts,
                    room_id,
                }),
            ) => {
                query_builder.push(" AND (rs.member_count, r.created_ts, r.room_id) < (");
                query_builder.push_bind(member_count);
                query_builder.push(", ");
                query_builder.push_bind(created_ts);
                query_builder.push(", ");
                query_builder.push_bind(room_id);
                query_builder.push(")");
            }
            (_, None) => { /* No cursor */ }
            _ => { /* Mismatched cursor and order_by, treat as no cursor */ }
        };

        query_builder.push(" GROUP BY r.room_id, rs.member_count, rs.is_encrypted ");

        match order_by {
            RoomSearchOrder::Created => {
                query_builder.push(" ORDER BY r.created_ts DESC, r.room_id DESC");
            }
            RoomSearchOrder::Name => {
                query_builder.push(" ORDER BY r.name DESC, r.created_ts DESC, r.room_id DESC");
            }
            RoomSearchOrder::Size => {
                query_builder
                    .push(" ORDER BY rs.member_count DESC, r.created_ts DESC, r.room_id DESC");
            }
        }

        query_builder.push(" LIMIT ");
        query_builder.push_bind(limit + 1); // Fetch one extra to check for next_batch

        let rows: Vec<RoomWithMembersRecord> = query_builder
            .build_query_as()
            .fetch_all(&*self.pool)
            .await?;

        let next_batch = if rows.len() > limit as usize {
            rows.get(limit as usize).map(|last_room| {
                let cursor = match order_by {
                    RoomSearchOrder::Created => RoomSearchCursor::Created {
                        created_ts: last_room.created_ts,
                        room_id: last_room.room_id.clone(),
                    },
                    RoomSearchOrder::Name => RoomSearchCursor::Name {
                        name: last_room.name.clone(),
                        created_ts: last_room.created_ts,
                        room_id: last_room.room_id.clone(),
                    },
                    RoomSearchOrder::Size => RoomSearchCursor::Size {
                        member_count: last_room.member_count.unwrap_or(0),
                        created_ts: last_room.created_ts,
                        room_id: last_room.room_id.clone(),
                    },
                };
                encode_room_search_cursor(&cursor)
            })
        } else {
            None
        };

        let rooms = rows
            .into_iter()
            .take(limit as usize)
            .map(|row| {
                (
                    Room {
                        room_id: row.room_id.clone(),
                        name: row.name.clone(),
                        topic: row.topic.clone(),
                        avatar_url: row.avatar_url.clone(),
                        canonical_alias: row.canonical_alias.clone(),
                        join_rule: row
                            .join_rule
                            .clone()
                            .unwrap_or_else(|| DEFAULT_JOIN_RULE.to_string()),
                        creator_user_id: row.creator.clone(),
                        room_version: row
                            .room_version
                            .clone()
                            .unwrap_or_else(|| DEFAULT_ROOM_VERSION.to_string()),
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
                    },
                    row.joined_members.unwrap_or(0),
                )
            })
            .collect();
        Ok((rooms, next_batch))
    }

    pub async fn get_user_rooms(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<String> = sqlx::query_scalar::<_, String>(
            r#"
            SELECT room_id FROM room_memberships WHERE user_id = $1 AND membership = 'join'
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn update_room_name(&self, room_id: &str, name: &str) -> Result<(), sqlx::Error> {
        Self::update_room_name_with_executor(&*self.pool, room_id, name).await
    }

    pub async fn update_room_name_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        room_id: &str,
        name: &str,
    ) -> Result<(), sqlx::Error> {
        Self::update_room_name_with_executor(&mut **tx, room_id, name).await
    }

    async fn update_room_name_with_executor<'a, E>(
        executor: E,
        room_id: &str,
        name: &str,
    ) -> Result<(), sqlx::Error>
    where
        E: sqlx::Executor<'a, Database = Postgres>,
    {
        sqlx::query(
            r#"
            UPDATE rooms SET name = $1 WHERE room_id = $2
            "#,
        )
        .bind(name)
        .bind(room_id)
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn update_room_topic(&self, room_id: &str, topic: &str) -> Result<(), sqlx::Error> {
        Self::update_room_topic_with_executor(&*self.pool, room_id, topic).await
    }

    pub async fn update_room_topic_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        room_id: &str,
        topic: &str,
    ) -> Result<(), sqlx::Error> {
        Self::update_room_topic_with_executor(&mut **tx, room_id, topic).await
    }

    async fn update_room_topic_with_executor<'a, E>(
        executor: E,
        room_id: &str,
        topic: &str,
    ) -> Result<(), sqlx::Error>
    where
        E: sqlx::Executor<'a, Database = Postgres>,
    {
        sqlx::query(
            r#"
            UPDATE rooms SET topic = $1 WHERE room_id = $2
            "#,
        )
        .bind(topic)
        .bind(room_id)
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn update_room_avatar(
        &self,
        room_id: &str,
        avatar_url: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE rooms SET avatar_url = $1 WHERE room_id = $2
            "#,
        )
        .bind(avatar_url)
        .bind(room_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_canonical_alias(
        &self,
        room_id: &str,
        alias: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE rooms SET canonical_alias = $1 WHERE room_id = $2
            "#,
        )
        .bind(alias)
        .bind(room_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_canonical_alias(
        &self,
        room_id: &str,
        alias: &str,
    ) -> Result<(), sqlx::Error> {
        self.set_canonical_alias(room_id, Some(alias)).await
    }

    pub async fn increment_member_count(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE room_summaries
            SET member_count = member_count + 1,
                joined_member_count = joined_member_count + 1,
                updated_ts = $2
            WHERE room_id = $1
            "#,
        )
        .bind(room_id)
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn decrement_member_count(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE room_summaries
            SET member_count = GREATEST(member_count - 1, 0),
                joined_member_count = GREATEST(joined_member_count - 1, 0),
                updated_ts = $2
            WHERE room_id = $1
            "#,
        )
        .bind(room_id)
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_room_count(&self) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COALESCE(COUNT(*), 0) FROM rooms
            "#,
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
    }

    pub async fn set_room_visibility(
        &self,
        room_id: &str,
        visibility: &str,
    ) -> Result<(), sqlx::Error> {
        let visibility_value = match visibility {
            "public" => "public",
            "private" => "private",
            _ => "private",
        };
        sqlx::query(
            r#"
            UPDATE rooms SET visibility = $1 WHERE room_id = $2
            "#,
        )
        .bind(visibility_value)
        .bind(room_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_room_alias(
        &self,
        room_id: &str,
        alias: &str,
        _created_by: &str,
    ) -> Result<(), sqlx::Error> {
        let creation_ts = chrono::Utc::now().timestamp_millis();
        let server_name = alias
            .rsplit_once(':')
            .map(|(_, server_name)| server_name)
            .filter(|server_name| !server_name.is_empty())
            .unwrap_or("localhost");
        sqlx::query(
            r#"
            INSERT INTO room_aliases (room_alias, room_id, server_name, created_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (room_alias) DO UPDATE SET
                room_id = EXCLUDED.room_id,
                created_ts = EXCLUDED.created_ts
            "#,
        )
        .bind(alias)
        .bind(room_id)
        .bind(server_name)
        .bind(creation_ts)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove_room_alias(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM room_aliases WHERE room_id = $1
            "#,
        )
        .bind(room_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove_room_alias_by_name(&self, alias: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM room_aliases WHERE room_alias = $1
            "#,
        )
        .bind(alias)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_room(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM rooms WHERE room_id = $1
            "#,
        )
        .bind(room_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn shutdown_room(&self, room_id: &str) -> Result<(), sqlx::Error> {
        // Mark room as inactive or delete it. For simplicity, we delete it from directory
        // and mark its name to indicate it's shutdown.
        sqlx::query(
            "UPDATE rooms SET is_public = false, name = COALESCE(name, '') || ' (SHUTDOWN)' WHERE room_id = $1",
        )
        .bind(room_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_room_version(&self, room_id: &str, version: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE rooms SET room_version = $1 WHERE room_id = $2")
            .bind(version)
            .bind(room_id)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    pub async fn copy_room_state(
        &self,
        source_room_id: &str,
        target_room_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO room_state_events (room_id, type, state_key, content, sender, origin_server_ts)
            SELECT $1, type, state_key, content, sender, origin_server_ts
            FROM room_state_events
            WHERE room_id = $2
            ON CONFLICT DO NOTHING
            "#
        )
        .bind(target_room_id)
        .bind(source_room_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_room_alias(&self, room_id: &str) -> Result<Option<String>, sqlx::Error> {
        let result: Option<(String,)> = sqlx::query_as(
            r#"
            SELECT room_alias FROM room_aliases WHERE room_id = $1 LIMIT 1
            "#,
        )
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.map(|r| r.0))
    }

    pub async fn get_room_aliases(&self, room_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let results: Vec<(String,)> = sqlx::query_as(
            r#"
            SELECT room_alias FROM room_aliases WHERE room_id = $1
            "#,
        )
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;
        Ok(results.into_iter().map(|r| r.0).collect())
    }

    pub async fn get_room_by_alias(&self, alias: &str) -> Result<Option<String>, sqlx::Error> {
        let result: Option<(String,)> = sqlx::query_as(
            r#"
            SELECT room_id FROM room_aliases WHERE room_alias = $1
            "#,
        )
        .bind(alias)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.map(|r| r.0))
    }

    pub async fn is_room_in_directory(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        let result: Option<(bool,)> = sqlx::query_as(
            r#"
            SELECT is_public FROM room_directory WHERE room_id = $1
            "#,
        )
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.map(|r| r.0).unwrap_or(false))
    }

    pub async fn set_room_directory(
        &self,
        room_id: &str,
        is_public: bool,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"
            INSERT INTO room_directory (room_id, is_public, added_ts)
            VALUES ($1, $2, $3)
            ON CONFLICT (room_id) DO UPDATE SET is_public = EXCLUDED.is_public
            "#,
        )
        .bind(room_id)
        .bind(is_public)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            UPDATE rooms SET is_public = $1 WHERE room_id = $2
            "#,
        )
        .bind(is_public)
        .bind(room_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove_room_directory(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM room_directory WHERE room_id = $1
            "#,
        )
        .bind(room_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_room_account_data(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
        content: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            r#"
            INSERT INTO room_account_data (user_id, room_id, data_type, data, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $5)
            ON CONFLICT (user_id, room_id, data_type) DO UPDATE SET data = EXCLUDED.data, updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(user_id)
        .bind(room_id)
        .bind(event_type)
        .bind(content)
        .bind(now)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_read_marker(
        &self,
        room_id: &str,
        user_id: &str,
        event_id: &str,
    ) -> Result<(), sqlx::Error> {
        let now: i64 = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"
            INSERT INTO read_markers (room_id, user_id, event_id, marker_type, created_ts, updated_ts)
            VALUES ($1, $2, $3, 'm.fully_read', $4, $4)
            ON CONFLICT (room_id, user_id, marker_type) DO UPDATE SET event_id = EXCLUDED.event_id, updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .bind(event_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// Update read marker with specific marker type
    /// Supports: m.fully_read, m.private_read, m.marked_unread
    pub async fn update_read_marker_with_type(
        &self,
        room_id: &str,
        user_id: &str,
        event_id: &str,
        marker_type: &str,
    ) -> Result<(), sqlx::Error> {
        let now: i64 = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"
            INSERT INTO read_markers (room_id, user_id, event_id, marker_type, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $5)
            ON CONFLICT (room_id, user_id, marker_type) DO UPDATE SET event_id = EXCLUDED.event_id, updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .bind(event_id)
        .bind(marker_type)
        .bind(now)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// Get read marker for a specific type
    pub async fn get_read_marker(
        &self,
        room_id: &str,
        user_id: &str,
        marker_type: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        let result = sqlx::query_as::<_, (String,)>(
            r#"
            SELECT event_id FROM read_markers
            WHERE room_id = $1 AND user_id = $2 AND marker_type = $3
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .bind(marker_type)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.map(|r| r.0))
    }

    /// Get all read markers for a user in a room
    pub async fn get_all_read_markers(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<std::collections::HashMap<String, String>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String)>(
            r#"
            SELECT marker_type, event_id FROM read_markers
            WHERE room_id = $1 AND user_id = $2
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().collect())
    }

    pub async fn add_receipt(
        &self,
        user_id: &str,
        _sent_to: &str,
        room_id: &str,
        event_id: &str,
        receipt_type: &str,
        data: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        let now: i64 = chrono::Utc::now().timestamp_millis();
        let receipt_data = if data.is_object() {
            data.clone()
        } else {
            json!({})
        };

        sqlx::query(
            r#"
            DELETE FROM event_receipts
            WHERE room_id = $1
              AND user_id = $2
              AND receipt_type = $3
              AND event_id <> $4
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .bind(receipt_type)
        .bind(event_id)
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO event_receipts (event_id, room_id, user_id, receipt_type, ts, data, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $5, $5)
            ON CONFLICT (event_id, room_id, user_id, receipt_type) DO UPDATE
            SET ts = EXCLUDED.ts, data = EXCLUDED.data, updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(event_id)
        .bind(room_id)
        .bind(user_id)
        .bind(receipt_type)
        .bind(now)
        .bind(receipt_data)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_receipts(
        &self,
        room_id: &str,
        receipt_type: &str,
        event_id: &str,
    ) -> Result<Vec<Receipt>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String, String, i64, serde_json::Value)>(
            r#"
            SELECT user_id, event_id, receipt_type, ts, data FROM event_receipts
            WHERE room_id = $1 AND receipt_type = $2 AND event_id = $3
            "#,
        )
        .bind(room_id)
        .bind(receipt_type)
        .bind(event_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(user_id, event_id, receipt_type, ts, data)| Receipt {
                user_id,
                event_id,
                receipt_type,
                ts,
                data,
            })
            .collect())
    }

    pub async fn get_rooms_map(
        &self,
        room_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Room>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let rooms = self.get_rooms_batch(room_ids).await?;

        Ok(rooms.into_iter().map(|r| (r.room_id.clone(), r)).collect())
    }

    pub async fn get_rooms_with_member_counts(
        &self,
        room_ids: &[String],
    ) -> Result<std::collections::HashMap<String, (Room, i64)>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let rows: Vec<RoomWithMembersRecord> = sqlx::query_as(
            r#"
            SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias, r.join_rules, r.creator,
                   r.room_version, r.is_public, rs.member_count as member_count, rs.is_encrypted as is_encrypted, r.history_visibility,
                   r.created_ts, COUNT(rm.user_id) as joined_members
            FROM rooms r
            LEFT JOIN room_memberships rm ON r.room_id = rm.room_id AND rm.membership = 'join'
            WHERE r.room_id = ANY($1)
            LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
            GROUP BY r.room_id, rs.member_count, rs.is_encrypted
            "#,
        )
        .bind(room_ids)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|row| {
                let room = Room {
                    room_id: row.room_id.clone(),
                    name: row.name.clone(),
                    topic: row.topic.clone(),
                    avatar_url: row.avatar_url.clone(),
                    canonical_alias: row.canonical_alias.clone(),
                    join_rule: row
                        .join_rule
                        .clone()
                        .unwrap_or_else(|| DEFAULT_JOIN_RULE.to_string()),
                    creator_user_id: row.creator.clone(),
                    room_version: row
                        .room_version
                        .clone()
                        .unwrap_or_else(|| DEFAULT_ROOM_VERSION.to_string()),
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
                };
                (row.room_id.clone(), (room, row.joined_members.unwrap_or(0)))
            })
            .collect())
    }

    pub async fn check_rooms_exist_batch(
        &self,
        room_ids: &[String],
    ) -> Result<std::collections::HashSet<String>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashSet::new());
        }

        let rows: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT room_id FROM rooms WHERE room_id = ANY($1)
            "#,
        )
        .bind(room_ids)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().collect())
    }

    /// 清理异常数据（孤儿数据、空房间等）
    ///
    /// # 参数
    ///
    /// * `min_age_ms` - 房间最小存活时间（毫秒），小于此时间的空房间不会被清理。默认 24h。
    pub async fn cleanup_abnormal_data(
        &self,
        min_age_ms: Option<i64>,
    ) -> Result<serde_json::Value, sqlx::Error> {
        let min_age = min_age_ms.unwrap_or(24 * 60 * 60 * 1000);
        let now = chrono::Utc::now().timestamp_millis();
        let cutoff = now - min_age;

        let mut results = serde_json::Map::new();

        // 1. 清理没有成员且存活超过 min_age 的房间
        let deleted_empty_rooms = sqlx::query(
            r#"
            DELETE FROM rooms
            WHERE created_ts < $1
            AND NOT EXISTS (
                SELECT 1 FROM room_memberships
                WHERE room_memberships.room_id = rooms.room_id
                AND membership = 'join'
            )
            "#,
        )
        .bind(cutoff)
        .execute(&*self.pool)
        .await?
        .rows_affected();
        results.insert(
            "deleted_empty_rooms".to_string(),
            json!(deleted_empty_rooms),
        );

        // 2. 清理孤儿事件 (events 指向不存在的 rooms)
        let deleted_orphan_events = sqlx::query(
            r#"
            DELETE FROM events
            WHERE NOT EXISTS (SELECT 1 FROM rooms WHERE rooms.room_id = events.room_id)
            "#,
        )
        .execute(&*self.pool)
        .await?
        .rows_affected();
        results.insert(
            "deleted_orphan_events".to_string(),
            json!(deleted_orphan_events),
        );

        // 3. 清理孤儿成员关系
        let deleted_orphan_memberships = sqlx::query(
            r#"
            DELETE FROM room_memberships
            WHERE NOT EXISTS (SELECT 1 FROM rooms WHERE rooms.room_id = room_memberships.room_id)
            "#,
        )
        .execute(&*self.pool)
        .await?
        .rows_affected();
        results.insert(
            "deleted_orphan_memberships".to_string(),
            json!(deleted_orphan_memberships),
        );

        // 4. 清理孤儿状态
        let deleted_orphan_state = sqlx::query(
            r#"
            DELETE FROM room_state_events
            WHERE NOT EXISTS (SELECT 1 FROM rooms WHERE rooms.room_id = room_state_events.room_id)
            "#,
        )
        .execute(&*self.pool)
        .await?
        .rows_affected();
        results.insert(
            "deleted_orphan_state".to_string(),
            json!(deleted_orphan_state),
        );

        Ok(serde_json::Value::Object(results))
    }

    pub async fn get_public_rooms_with_aliases(
        &self,
        limit: i64,
        since_ts: Option<i64>,
        since_room_id: Option<&str>,
    ) -> Result<Vec<(Room, Vec<String>)>, sqlx::Error> {
        let rows: Vec<RoomRecord> = if let (Some(ts), Some(room_id)) = (since_ts, since_room_id) {
            sqlx::query_as(
                r#"
                SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias, r.join_rules, r.creator, r.room_version,
                      r.is_public, rs.member_count as member_count, rs.is_encrypted as is_encrypted, r.history_visibility, r.created_ts
                FROM rooms r
                LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
                WHERE r.is_public = TRUE
                  AND (r.created_ts < $2 OR (r.created_ts = $2 AND r.room_id < $3))
                ORDER BY r.created_ts DESC, r.room_id DESC
                LIMIT $1
                "#,
            )
            .bind(limit)
            .bind(ts)
            .bind(room_id)
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query_as(
                r#"
                SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias, r.join_rules, r.creator, r.room_version,
                      r.is_public, rs.member_count as member_count, rs.is_encrypted as is_encrypted, r.history_visibility, r.created_ts
                FROM rooms r
                LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
                WHERE r.is_public = TRUE
                ORDER BY r.created_ts DESC, r.room_id DESC
                LIMIT $1
                "#,
            )
            .bind(limit)
            .fetch_all(&*self.pool)
            .await?
        };

        let room_ids: Vec<String> = rows.iter().map(|r| r.room_id.clone()).collect();
        let aliases = self.get_room_aliases_batch(&room_ids).await?;

        Ok(rows
            .iter()
            .map(|row| {
                let room = Room {
                    room_id: row.room_id.clone(),
                    name: row.name.clone(),
                    topic: row.topic.clone(),
                    avatar_url: row.avatar_url.clone(),
                    canonical_alias: row.canonical_alias.clone(),
                    join_rule: row
                        .join_rule
                        .clone()
                        .unwrap_or_else(|| DEFAULT_JOIN_RULE.to_string()),
                    creator_user_id: row.creator.clone(),
                    room_version: row
                        .room_version
                        .clone()
                        .unwrap_or_else(|| DEFAULT_ROOM_VERSION.to_string()),
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
                };
                let room_aliases = aliases.get(&row.room_id).cloned().unwrap_or_default();
                (room, room_aliases)
            })
            .collect())
    }

    pub async fn get_room_aliases_batch(
        &self,
        room_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<String>>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let rows: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT room_id, room_alias FROM room_aliases WHERE room_id = ANY($1)
            "#,
        )
        .bind(room_ids)
        .fetch_all(&*self.pool)
        .await?;

        let mut result: std::collections::HashMap<String, Vec<String>> =
            room_ids.iter().map(|id| (id.clone(), Vec::new())).collect();

        for (room_id, room_alias) in rows {
            if let Some(aliases) = result.get_mut(&room_id) {
                aliases.push(room_alias);
            }
        }

        Ok(result)
    }

    pub async fn get_rooms_by_aliases_batch(
        &self,
        aliases: &[String],
    ) -> Result<std::collections::HashMap<String, String>, sqlx::Error> {
        if aliases.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let rows: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT room_alias, room_id FROM room_aliases WHERE room_alias = ANY($1)
            "#,
        )
        .bind(aliases)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().collect())
    }

    pub async fn increment_member_counts_batch(
        &self,
        room_ids: &[String],
    ) -> Result<u64, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(0);
        }

        let result = sqlx::query(
            r#"
            UPDATE room_summaries
            SET member_count = member_count + 1,
                joined_member_count = joined_member_count + 1,
                updated_ts = $2
            WHERE room_id = ANY($1)
            "#,
        )
        .bind(room_ids)
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn decrement_member_counts_batch(
        &self,
        room_ids: &[String],
    ) -> Result<u64, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(0);
        }

        let result = sqlx::query(
            r#"
            UPDATE room_summaries
            SET member_count = GREATEST(member_count - 1, 0),
                joined_member_count = GREATEST(joined_member_count - 1, 0),
                updated_ts = $2
            WHERE room_id = ANY($1)
            "#,
        )
        .bind(room_ids)
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_room_struct() {
        let room = Room {
            room_id: "!room:example.com".to_string(),
            name: Some("Test Room".to_string()),
            topic: Some("A test room".to_string()),
            avatar_url: Some("mxc://example.com/avatar".to_string()),
            canonical_alias: Some("#test:example.com".to_string()),
            join_rule: "invite".to_string(),
            creator_user_id: Some("@alice:example.com".to_string()),
            room_version: "6".to_string(),
            encryption: Some("m.megolm.v1.aes-sha2".to_string()),
            is_public: false,
            member_count: 5,
            history_visibility: "joined".to_string(),
            created_ts: 1234567890,
            is_federatable: true,
            is_spotlight: false,
            is_flagged: false,
        };

        assert_eq!(room.room_id, "!room:example.com");
        assert_eq!(room.name, Some("Test Room".to_string()));
        assert_eq!(room.member_count, 5);
    }

    #[test]
    fn test_room_minimal() {
        let room = Room {
            room_id: "!minimal:example.com".to_string(),
            name: None,
            topic: None,
            avatar_url: None,
            canonical_alias: None,
            join_rule: DEFAULT_JOIN_RULE.to_string(),
            creator_user_id: Some("@bob:example.com".to_string()),
            room_version: DEFAULT_ROOM_VERSION.to_string(),
            encryption: None,
            is_public: true,
            member_count: 1,
            history_visibility: DEFAULT_HISTORY_VISIBILITY.to_string(),
            created_ts: 0,
            is_federatable: true,
            is_spotlight: false,
            is_flagged: false,
        };

        assert!(room.name.is_none());
        assert!(room.encryption.is_none());
        assert!(room.is_public);
    }

    #[test]
    fn test_room_serialization() {
        let room = Room {
            room_id: "!serialize:example.com".to_string(),
            name: Some("Serialize Test".to_string()),
            topic: None,
            avatar_url: None,
            canonical_alias: None,
            join_rule: "public".to_string(),
            creator_user_id: Some("@test:example.com".to_string()),
            room_version: "9".to_string(),
            encryption: None,
            is_public: true,
            member_count: 10,
            history_visibility: "shared".to_string(),
            created_ts: 1234567890,
            is_federatable: true,
            is_spotlight: false,
            is_flagged: false,
        };

        let json = serde_json::to_string(&room).unwrap();
        assert!(json.contains("!serialize:example.com"));
        assert!(json.contains("Serialize Test"));
    }

    #[test]
    fn test_room_encrypted() {
        let room = Room {
            room_id: "!encrypted:example.com".to_string(),
            name: Some("Encrypted Room".to_string()),
            topic: None,
            avatar_url: None,
            canonical_alias: None,
            join_rule: "invite".to_string(),
            creator_user_id: Some("@admin:example.com".to_string()),
            room_version: "6".to_string(),
            encryption: Some("m.megolm.v1.aes-sha2".to_string()),
            is_public: false,
            member_count: 3,
            history_visibility: "invited".to_string(),
            created_ts: 1234567890,
            is_federatable: true,
            is_spotlight: false,
            is_flagged: false,
        };

        assert!(room.encryption.is_some());
        let enc = room.encryption.unwrap();
        assert_eq!(enc, "m.megolm.v1.aes-sha2");
    }

    #[test]
    fn test_default_constants() {
        assert_eq!(DEFAULT_JOIN_RULE, "invite");
        assert_eq!(DEFAULT_HISTORY_VISIBILITY, "joined");
    }
}

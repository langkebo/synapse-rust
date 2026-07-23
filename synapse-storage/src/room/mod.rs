pub mod admin;
pub(crate) mod api;
pub(crate) mod models;

pub use api::RoomStoreApi;
pub use models::*;

// Room domain group — re-exports room-related storage modules under `room::`.
// Consumers should prefer `synapse_storage::room::RoomMemberStorage` over the
// flat `synapse_storage::RoomMemberStorage`.
pub use crate::membership::{MemberStoreApi, RoomMember, RoomMemberStorage, UserRoomMembership};
pub use crate::room_account_data::{RoomAccountDataRecord, RoomAccountDataStorage, RoomAccountDataStoreApi};
pub use crate::state_groups::StateGroupStoreApi;
pub use crate::thread::{
    CreateThreadReplyParams, CreateThreadRootParams, ThreadListParams, ThreadReadReceipt, ThreadRelation, ThreadReply,
    ThreadRoot, ThreadStatistics, ThreadStorage, ThreadStoreApi, ThreadSubscription, ThreadSummary, ThreadWithReplies,
};

// P7.3: relations, retention, room_summary, room_tag, beacon, widget,
// burn_after_read, and friend_room are room-related storage modules — group
// them under `room::` so they are flat-re-exported via `pub use room::*;`
// rather than via explicit flat re-exports in lib.rs.
#[cfg(feature = "beacons")]
pub use crate::beacon::{
    BeaconInfo, BeaconInfoWithLocations, BeaconLocation, BeaconStorage, BeaconStoreApi, CreateBeaconInfoParams,
    CreateBeaconLocationParams,
};
#[cfg(feature = "burn-after-read")]
pub use crate::burn_after_read::*;
#[cfg(feature = "friends")]
pub use crate::friend_room::{
    AddFriendToGroupParams, CreateFriendGroupParams, DirectRoomFallbackLink, DmPartnerRecord, FriendDmLink,
    FriendRequestRecord, FriendRoomStorage, FriendRoomStoreApi, RemoveFriendFromGroupParams, RenameFriendGroupParams,
};
pub use crate::relations::*;
pub use crate::retention::*;
pub use crate::room_summary::*;
pub use crate::room_tag::*;
#[cfg(feature = "widgets")]
pub use crate::widget::{CreateWidgetParams, Widget, WidgetPermission, WidgetSession, WidgetStorage, WidgetStoreApi};

use serde_json::json;
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use tracing;

use synapse_common::room_versions::DEFAULT_ROOM_VERSION;

impl RoomStorage {
    pub(crate) fn encryption_from_is_encrypted(is_encrypted: Option<bool>) -> Option<String> {
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
        Self::create_room_with_executor(&*self.pool, room_id, creator, join_rule, version, is_public).await
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
        Self::create_room_with_executor(&mut **tx, room_id, creator, join_rule, version, is_public).await
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
        tracing::info!(room_id = %room_id, creator = %creator, join_rule = %join_rule, is_public = is_public, "Creating room");
        let now = current_timestamp_millis();
        sqlx::query(
            r"
            INSERT INTO rooms (room_id, creator, join_rules, room_version, is_public, history_visibility, created_ts, last_activity_ts)
            VALUES ($1, $2, $3, $4, $5, 'joined', $6, $6)
            ",
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
        tracing::debug!(room_id = %room_id, "Querying room");
        let row = sqlx::query_as::<_, RoomRecord>(
            r"
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
            ",
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
                join_rule: row.join_rule.unwrap_or_else(|| DEFAULT_JOIN_RULE.to_string()),
                creator_user_id: row.creator,
                room_version: row.room_version.unwrap_or_else(|| DEFAULT_ROOM_VERSION.to_string()),
                encryption: Self::encryption_from_is_encrypted(row.is_encrypted),
                is_public: row.is_public.unwrap_or(false),
                member_count: row.member_count.unwrap_or(0),
                history_visibility: row.history_visibility.unwrap_or_else(|| DEFAULT_HISTORY_VISIBILITY.to_string()),
                created_ts: row.created_ts,
                is_federatable: true,
                is_spotlight: false,
                is_flagged: false,
            }))
        } else {
            tracing::warn!(room_id = %room_id, "Room not found");
            Ok(None)
        }
    }

    pub async fn get_rooms_batch(&self, room_ids: &[String]) -> Result<Vec<Room>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(Vec::new());
        }

        let rows: Vec<RoomRecord> = sqlx::query_as(
            r"
            SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias, r.join_rules, r.creator, r.room_version,
                  r.is_public, rs.member_count as member_count, rs.is_encrypted as is_encrypted, r.history_visibility, r.created_ts
            FROM rooms r
            LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
            WHERE r.room_id = ANY($1)
            ",
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

    pub async fn get_room_creator(&self, room_id: &str) -> Result<Option<String>, sqlx::Error> {
        let result: Option<(String,)> = sqlx::query_as(
            r"
            SELECT creator FROM rooms WHERE room_id = $1
            ",
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

    /// Paginated public rooms list. Uses Keyset pagination (created_ts, room_id).
    pub async fn get_public_rooms_paginated(
        &self,
        limit: i64,
        since_ts: Option<i64>,
        since_room_id: Option<&str>,
    ) -> Result<Vec<Room>, sqlx::Error> {
        let rows: Vec<RoomRecord> = if let (Some(ts), Some(room_id)) = (since_ts, since_room_id) {
            sqlx::query_as(
                r"
                SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias, r.join_rules, r.creator, r.room_version,
                      r.is_public, rs.member_count as member_count, rs.is_encrypted as is_encrypted, r.history_visibility, r.created_ts
                FROM rooms r
                LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
                WHERE r.is_public = TRUE AND (r.created_ts < $2 OR (r.created_ts = $2 AND r.room_id < $3))
                ORDER BY r.created_ts DESC, r.room_id DESC
                LIMIT $1
                ",
            )
            .bind(limit)
            .bind(ts)
            .bind(room_id)
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query_as(
                r"
                SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias, r.join_rules, r.creator, r.room_version,
                      r.is_public, rs.member_count as member_count, rs.is_encrypted as is_encrypted, r.history_visibility, r.created_ts
                FROM rooms r
                LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
                WHERE r.is_public = TRUE
                ORDER BY r.created_ts DESC, r.room_id DESC
                LIMIT $1
                ",
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

    /// Returns the total number of public rooms, for the `total_room_count_estimate` field.
    pub async fn count_public_rooms(&self) -> Result<i64, sqlx::Error> {
        let count: (i64,) = sqlx::query_as(
            r"
            SELECT COUNT(*) FROM rooms WHERE is_public = TRUE
            ",
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
            r"
            SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias, r.join_rules, r.creator,
                r.room_version, r.is_public, rs.member_count as member_count, rs.is_encrypted as is_encrypted, r.history_visibility,
                r.created_ts, COUNT(rm.user_id) as joined_members
            FROM rooms r
            LEFT JOIN room_memberships rm ON r.room_id = rm.room_id AND rm.membership = 'join'
            LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
            ",
        );

        query_builder.push(" WHERE 1 = 1 ");

        match (order_by, from) {
            (RoomSearchOrder::Created, Some(RoomSearchCursor::Created { created_ts, room_id })) => {
                query_builder.push(" AND (r.created_ts, r.room_id) < (");
                query_builder.push_bind(created_ts);
                query_builder.push(", ");
                query_builder.push_bind(room_id);
                query_builder.push(")");
            }
            (RoomSearchOrder::Name, Some(RoomSearchCursor::Name { name, created_ts, room_id })) => {
                query_builder.push(" AND (r.name, r.created_ts, r.room_id) < (");
                query_builder.push_bind(name);
                query_builder.push(", ");
                query_builder.push_bind(created_ts);
                query_builder.push(", ");
                query_builder.push_bind(room_id);
                query_builder.push(")");
            }
            (RoomSearchOrder::Size, Some(RoomSearchCursor::Size { member_count, created_ts, room_id })) => {
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
                query_builder.push(" ORDER BY rs.member_count DESC, r.created_ts DESC, r.room_id DESC");
            }
        }

        query_builder.push(" LIMIT ");
        query_builder.push_bind(limit + 1); // Fetch one extra to check for next_batch

        let rows: Vec<RoomWithMembersRecord> = query_builder.build_query_as().fetch_all(&*self.pool).await?;

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
                    },
                    row.joined_members.unwrap_or(0),
                )
            })
            .collect();
        Ok((rooms, next_batch))
    }

    pub async fn get_user_rooms(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<String> = sqlx::query_scalar::<_, String>(
            r"
            SELECT room_id FROM room_memberships WHERE user_id = $1 AND membership = 'join'
            LIMIT 1000
            ",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn search_rooms_for_user(
        &self,
        user_id: &str,
        search_pattern: &str,
        limit: i64,
    ) -> Result<Vec<(String, Option<String>, Option<String>, Option<String>, bool)>, sqlx::Error> {
        sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<String>, bool)>(
            r"
            SELECT room_id, name, topic, avatar_url, is_public
            FROM rooms
            WHERE
                (LOWER(name) LIKE $1 OR LOWER(topic) LIKE $1)
                AND (
                    is_public = true
                    OR EXISTS (
                        SELECT 1
                        FROM room_memberships
                        WHERE room_memberships.room_id = rooms.room_id
                          AND room_memberships.user_id = $2
                          AND room_memberships.membership = 'join'
                    )
                )
            ORDER BY name
            LIMIT $3
            ",
        )
        .bind(search_pattern)
        .bind(user_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_user_room_list_summary(
        &self,
        user_id: &str,
    ) -> Result<Vec<(String, String, String, String)>, sqlx::Error> {
        sqlx::query_as::<_, (String, String, String, String)>(
            r"
            SELECT rm.room_id, rm.membership,
                   COALESCE(r.name, '') AS name,
                   COALESCE(r.avatar_url, '') AS avatar_url
            FROM room_memberships rm
            LEFT JOIN rooms r ON rm.room_id = r.room_id
            WHERE rm.user_id = $1
            ORDER BY rm.updated_ts DESC
            ",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_user_rooms_paginated(
        &self,
        user_id: &str,
        limit: i64,
        from_room_id: Option<&str>,
    ) -> Result<Vec<String>, sqlx::Error> {
        if let Some(room_id) = from_room_id {
            sqlx::query_scalar::<_, String>(
                r"
                SELECT room_id FROM room_memberships
                WHERE user_id = $1 AND membership = 'join' AND room_id > $2
                ORDER BY room_id ASC
                LIMIT $3
                ",
            )
            .bind(user_id)
            .bind(room_id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        } else {
            sqlx::query_scalar::<_, String>(
                r"
                SELECT room_id FROM room_memberships
                WHERE user_id = $1 AND membership = 'join'
                ORDER BY room_id ASC
                LIMIT $2
                ",
            )
            .bind(user_id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        }
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

    async fn update_room_name_with_executor<'a, E>(executor: E, room_id: &str, name: &str) -> Result<(), sqlx::Error>
    where
        E: sqlx::Executor<'a, Database = Postgres>,
    {
        sqlx::query(
            r"
            UPDATE rooms SET name = $1 WHERE room_id = $2
            ",
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

    async fn update_room_topic_with_executor<'a, E>(executor: E, room_id: &str, topic: &str) -> Result<(), sqlx::Error>
    where
        E: sqlx::Executor<'a, Database = Postgres>,
    {
        sqlx::query(
            r"
            UPDATE rooms SET topic = $1 WHERE room_id = $2
            ",
        )
        .bind(topic)
        .bind(room_id)
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn update_room_avatar(&self, room_id: &str, avatar_url: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE rooms SET avatar_url = $1 WHERE room_id = $2
            ",
        )
        .bind(avatar_url)
        .bind(room_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_canonical_alias(&self, room_id: &str, alias: Option<&str>) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE rooms SET canonical_alias = $1 WHERE room_id = $2
            ",
        )
        .bind(alias)
        .bind(room_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_join_rule_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        room_id: &str,
        join_rule: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE rooms SET join_rules = $1 WHERE room_id = $2
            ",
        )
        .bind(join_rule)
        .bind(room_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub async fn update_canonical_alias(&self, room_id: &str, alias: &str) -> Result<(), sqlx::Error> {
        self.set_canonical_alias(room_id, Some(alias)).await
    }

    pub async fn increment_member_count(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE room_summaries
            SET member_count = member_count + 1,
                joined_member_count = joined_member_count + 1,
                updated_ts = $2
            WHERE room_id = $1
            ",
        )
        .bind(room_id)
        .bind(current_timestamp_millis())
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn decrement_member_count(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE room_summaries
            SET member_count = GREATEST(member_count - 1, 0),
                joined_member_count = GREATEST(joined_member_count - 1, 0),
                updated_ts = $2
            WHERE room_id = $1
            ",
        )
        .bind(room_id)
        .bind(current_timestamp_millis())
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_room_count(&self) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar::<_, i64>(
            r"
            SELECT COALESCE(COUNT(*), 0) FROM rooms
            ",
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
    }

    pub async fn set_room_visibility(&self, room_id: &str, visibility: &str) -> Result<(), sqlx::Error> {
        // Normalize the visibility string and derive the boolean is_public
        // flag from it. `is_public` is what most callers actually read, so we
        // keep it in sync with the visibility column.
        let (visibility_value, is_public) = match visibility {
            "public" => ("public", true),
            "private" => ("private", false),
            _ => ("private", false),
        };
        sqlx::query(
            r"
            UPDATE rooms SET visibility = $1, is_public = $2 WHERE room_id = $3
            ",
        )
        .bind(visibility_value)
        .bind(is_public)
        .bind(room_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_room_alias(&self, room_id: &str, alias: &str, _created_by: &str) -> Result<(), sqlx::Error> {
        let creation_ts = current_timestamp_millis();
        let server_name = alias
            .rsplit_once(':')
            .map(|(_, server_name)| server_name)
            .filter(|server_name| !server_name.is_empty())
            .unwrap_or("localhost");
        sqlx::query(
            r"
            INSERT INTO room_aliases (room_alias, room_id, server_name, created_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (room_alias) DO UPDATE SET
                room_id = EXCLUDED.room_id,
                created_ts = EXCLUDED.created_ts
            ",
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
            r"
            DELETE FROM room_aliases WHERE room_id = $1
            ",
        )
        .bind(room_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove_room_alias_by_name(&self, alias: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            DELETE FROM room_aliases WHERE room_alias = $1
            ",
        )
        .bind(alias)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_room(&self, room_id: &str) -> Result<(), sqlx::Error> {
        tracing::info!(room_id = %room_id, "Deleting room");
        sqlx::query(
            r"
            DELETE FROM rooms WHERE room_id = $1
            ",
        )
        .bind(room_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn shutdown_room(&self, room_id: &str) -> Result<(), sqlx::Error> {
        tracing::info!(room_id = %room_id, "Shutting down room");
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

    pub async fn get_room_alias(&self, room_id: &str) -> Result<Option<String>, sqlx::Error> {
        let result: Option<(String,)> = sqlx::query_as(
            r"
            SELECT room_alias FROM room_aliases WHERE room_id = $1 LIMIT 1
            ",
        )
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.map(|r| r.0))
    }

    pub async fn get_room_aliases(&self, room_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let results: Vec<(String,)> = sqlx::query_as(
            r"
            SELECT room_alias FROM room_aliases WHERE room_id = $1
            ",
        )
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;
        Ok(results.into_iter().map(|r| r.0).collect())
    }

    pub async fn get_room_by_alias(&self, alias: &str) -> Result<Option<String>, sqlx::Error> {
        let result: Option<(String,)> = sqlx::query_as(
            r"
            SELECT room_id FROM room_aliases WHERE room_alias = $1
            ",
        )
        .bind(alias)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.map(|r| r.0))
    }

    pub async fn is_room_in_directory(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        let result: Option<(bool,)> = sqlx::query_as(
            r"
            SELECT is_public FROM room_directory WHERE room_id = $1
            ",
        )
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.is_some_and(|r| r.0))
    }

    pub async fn set_room_directory(&self, room_id: &str, is_public: bool) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        sqlx::query(
            r"
            INSERT INTO room_directory (room_id, is_public, added_ts)
            VALUES ($1, $2, $3)
            ON CONFLICT (room_id) DO UPDATE SET is_public = EXCLUDED.is_public
            ",
        )
        .bind(room_id)
        .bind(is_public)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r"
            UPDATE rooms SET is_public = $1 WHERE room_id = $2
            ",
        )
        .bind(is_public)
        .bind(room_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove_room_directory(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            DELETE FROM room_directory WHERE room_id = $1
            ",
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
            r"
            INSERT INTO room_account_data (user_id, room_id, data_type, data, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $5)
            ON CONFLICT (user_id, room_id, data_type) DO UPDATE SET data = EXCLUDED.data, updated_ts = EXCLUDED.updated_ts
            ",
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

    pub async fn update_read_marker(&self, room_id: &str, user_id: &str, event_id: &str) -> Result<(), sqlx::Error> {
        let now: i64 = current_timestamp_millis();
        sqlx::query(
            r"
            INSERT INTO read_markers (room_id, user_id, event_id, marker_type, created_ts, updated_ts)
            VALUES ($1, $2, $3, 'm.fully_read', $4, $4)
            ON CONFLICT (room_id, user_id, marker_type) DO UPDATE SET event_id = EXCLUDED.event_id, updated_ts = EXCLUDED.updated_ts
            ",
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
        let now: i64 = current_timestamp_millis();
        sqlx::query(
            r"
            INSERT INTO read_markers (room_id, user_id, event_id, marker_type, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $5)
            ON CONFLICT (room_id, user_id, marker_type) DO UPDATE SET event_id = EXCLUDED.event_id, updated_ts = EXCLUDED.updated_ts
            ",
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
            r"
            SELECT event_id FROM read_markers
            WHERE room_id = $1 AND user_id = $2 AND marker_type = $3
            ",
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
            r"
            SELECT marker_type, event_id FROM read_markers
            WHERE room_id = $1 AND user_id = $2
            ",
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
        let now: i64 = current_timestamp_millis();
        let receipt_data = if data.is_object() { data.clone() } else { json!({}) };

        sqlx::query(
            r"
            DELETE FROM event_receipts
            WHERE room_id = $1
              AND user_id = $2
              AND receipt_type = $3
              AND event_id <> $4
            ",
        )
        .bind(room_id)
        .bind(user_id)
        .bind(receipt_type)
        .bind(event_id)
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r"
            INSERT INTO event_receipts (event_id, room_id, user_id, receipt_type, ts, data, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $5, $5)
            ON CONFLICT (event_id, room_id, user_id, receipt_type) DO UPDATE
            SET ts = EXCLUDED.ts, data = EXCLUDED.data, updated_ts = EXCLUDED.updated_ts
            ",
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
            r"
            SELECT user_id, event_id, receipt_type, ts, data FROM event_receipts
            WHERE room_id = $1 AND receipt_type = $2 AND event_id = $3
            ",
        )
        .bind(room_id)
        .bind(receipt_type)
        .bind(event_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(user_id, event_id, receipt_type, ts, data)| Receipt { user_id, event_id, receipt_type, ts, data })
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
            r"
            SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias, r.join_rules, r.creator,
                   r.room_version, r.is_public, rs.member_count as member_count, rs.is_encrypted as is_encrypted, r.history_visibility,
                   r.created_ts, COUNT(rm.user_id) as joined_members
            FROM rooms r
            LEFT JOIN room_memberships rm ON r.room_id = rm.room_id AND rm.membership = 'join'
            WHERE r.room_id = ANY($1)
            LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
            GROUP BY r.room_id, rs.member_count, rs.is_encrypted
            ",
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
                };
                (row.room_id.clone(), (room, row.joined_members.unwrap_or(0)))
            })
            .collect())
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    async fn test_pool() -> Arc<Pool<Postgres>> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    #[allow(dead_code)]
    async fn ensure_test_room(pool: &Pool<Postgres>, room_id: &str, creator: &str) {
        let now = current_timestamp_millis();
        sqlx::query(
            r#"INSERT INTO rooms (room_id, creator, join_rules, room_version, is_public, history_visibility, created_ts, last_activity_ts)
               VALUES ($1, $2, 'invite', '10', false, 'joined', $3, $3)
               ON CONFLICT (room_id) DO NOTHING"#,
        )
        .bind(room_id)
        .bind(creator)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test room");
    }

    #[tokio::test]
    async fn test_create_room_returns_valid_room() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let room_id = format!("!create_test_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_room(&room_id).await;

        let room = storage
            .create_room(&room_id, "@creator:example.com", "invite", "10", false)
            .await
            .expect("create_room should succeed");

        assert_eq!(room.room_id, room_id);
        assert_eq!(room.creator_user_id, Some("@creator:example.com".to_string()));
        assert_eq!(room.join_rule, "invite");
        assert!(!room.is_public);

        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_get_room_found() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let room_id = format!("!get_test_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_room(&room_id).await;
        storage.create_room(&room_id, "@creator:example.com", "invite", "10", false).await.unwrap();

        let found = storage.get_room(&room_id).await.expect("get_room should succeed");
        assert!(found.is_some());
        assert_eq!(found.unwrap().room_id, room_id);

        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_get_room_not_found() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let result = storage.get_room("!nonexistent:example.com").await.expect("get_room should succeed");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_room_exists() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let room_id = format!("!exists_test_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_room(&room_id).await;
        storage.create_room(&room_id, "@creator:example.com", "invite", "10", false).await.unwrap();

        assert!(storage.room_exists(&room_id).await.expect("room_exists should succeed"));
        assert!(!storage.room_exists("!nope:example.com").await.expect("room_exists should succeed"));

        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_get_room_creator() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let room_id = format!("!creator_test_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_room(&room_id).await;
        storage.create_room(&room_id, "@thecreator:example.com", "invite", "10", false).await.unwrap();

        let creator = storage.get_room_creator(&room_id).await.expect("get_room_creator should succeed");
        assert_eq!(creator, Some("@thecreator:example.com".to_string()));

        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_get_rooms_batch() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let rid1 = format!("!batch1_{}:example.com", uuid::Uuid::new_v4());
        let rid2 = format!("!batch2_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_room(&rid1).await;
        let _ = storage.delete_room(&rid2).await;
        storage.create_room(&rid1, "@c:example.com", "invite", "10", false).await.unwrap();
        storage.create_room(&rid2, "@c:example.com", "invite", "10", false).await.unwrap();

        let rooms =
            storage.get_rooms_batch(&[rid1.clone(), rid2.clone()]).await.expect("get_rooms_batch should succeed");
        assert_eq!(rooms.len(), 2);

        let _ = storage.delete_room(&rid1).await;
        let _ = storage.delete_room(&rid2).await;
    }

    #[tokio::test]
    async fn test_update_room_name() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let room_id = format!("!name_test_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_room(&room_id).await;
        storage.create_room(&room_id, "@c:example.com", "invite", "10", false).await.unwrap();

        storage.update_room_name(&room_id, "New Room Name").await.expect("update_room_name should succeed");
        let room = storage.get_room(&room_id).await.unwrap().unwrap();
        assert_eq!(room.name, Some("New Room Name".to_string()));

        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_update_room_topic() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let room_id = format!("!topic_test_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_room(&room_id).await;
        storage.create_room(&room_id, "@c:example.com", "invite", "10", false).await.unwrap();

        storage.update_room_topic(&room_id, "New Topic").await.expect("update_room_topic should succeed");
        let room = storage.get_room(&room_id).await.unwrap().unwrap();
        assert_eq!(room.topic, Some("New Topic".to_string()));

        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_get_room_count() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let count = storage.get_room_count().await.expect("get_room_count should succeed");
        assert!(count >= 0);
    }

    #[tokio::test]
    async fn test_set_room_public_and_directory() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let room_id = format!("!pub_test_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_room(&room_id).await;
        storage.create_room(&room_id, "@c:example.com", "invite", "10", false).await.unwrap();
        assert!(!storage.is_room_in_directory(&room_id).await.unwrap());

        storage.set_room_visibility(&room_id, "public").await.expect("set_room_visibility should succeed");
        storage.set_room_directory(&room_id, true).await.expect("set_room_directory should succeed");
        let room = storage.get_room(&room_id).await.unwrap().unwrap();
        assert!(room.is_public);
        assert!(storage.is_room_in_directory(&room_id).await.unwrap());

        storage.set_room_visibility(&room_id, "private").await.expect("unset public should succeed");
        let room = storage.get_room(&room_id).await.unwrap().unwrap();
        assert!(!room.is_public);

        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_delete_room() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let room_id = format!("!delete_test_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_room(&room_id).await;
        storage.create_room(&room_id, "@c:example.com", "invite", "10", false).await.unwrap();
        assert!(storage.room_exists(&room_id).await.unwrap());

        storage.delete_room(&room_id).await.expect("delete_room should succeed");
        assert!(!storage.room_exists(&room_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_get_public_rooms() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let rooms = storage.get_public_rooms(5).await.expect("get_public_rooms should succeed");
        // All returned rooms should be public
        for room in &rooms {
            assert!(room.is_public);
        }
    }

    #[tokio::test]
    async fn test_count_public_rooms() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let count = storage.count_public_rooms().await.expect("count_public_rooms should succeed");
        assert!(count >= 0);
    }

    #[tokio::test]
    async fn test_get_public_rooms_paginated() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let rooms =
            storage.get_public_rooms_paginated(5, None, None).await.expect("get_public_rooms_paginated should succeed");
        assert!(rooms.len() <= 5);
        for room in &rooms {
            assert!(room.is_public);
        }
    }

    #[tokio::test]
    async fn test_room_alias_crud() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!alias_test_{}:example.com", suffix);
        let alias = format!("#mytestalias_{}:example.com", suffix);
        let _ = storage.delete_room(&room_id).await;
        storage.create_room(&room_id, "@c:example.com", "invite", "10", false).await.unwrap();

        storage.set_room_alias(&room_id, &alias, "@c:example.com").await.expect("set_room_alias should succeed");

        let aliases = storage.get_room_aliases(&room_id).await.expect("get_room_aliases should succeed");
        assert!(aliases.contains(&alias));

        let resolved = storage.get_room_by_alias(&alias).await.expect("get_room_by_alias should succeed");
        assert_eq!(resolved, Some(room_id.clone()));

        storage.remove_room_alias_by_name(&alias).await.expect("remove_room_alias_by_name should succeed");
        let after = storage.get_room_by_alias(&alias).await.unwrap();
        assert!(after.is_none());

        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_search_room_directory() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let results = storage.search_room_directory("test", 10).await.expect("search_room_directory should succeed");
        // Search may return 0 results — just verify it doesn't error
        assert!(results.len() <= 10);
    }

    #[tokio::test]
    async fn test_get_user_rooms() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let room_ids = storage.get_user_rooms("@testuser:example.com").await.expect("get_user_rooms should succeed");
        // Room membership may be empty for a test user — the .expect() above
        // already verifies the query itself succeeds without error.
        let _ = room_ids;
    }

    #[tokio::test]
    async fn test_update_room_avatar() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let room_id = format!("!avatar_test_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_room(&room_id).await;
        storage.create_room(&room_id, "@c:example.com", "invite", "10", false).await.unwrap();

        storage
            .update_room_avatar(&room_id, "mxc://example.com/avatar123")
            .await
            .expect("update_room_avatar should succeed");
        let room = storage.get_room(&room_id).await.unwrap().unwrap();
        assert_eq!(room.avatar_url.as_deref(), Some("mxc://example.com/avatar123"));

        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_set_canonical_alias_some_then_none() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let room_id = format!("!alias_set_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_room(&room_id).await;
        storage.create_room(&room_id, "@c:example.com", "invite", "10", false).await.unwrap();

        storage
            .set_canonical_alias(&room_id, Some("#alias:example.com"))
            .await
            .expect("set_canonical_alias Some should succeed");
        let room = storage.get_room(&room_id).await.unwrap().unwrap();
        assert_eq!(room.canonical_alias.as_deref(), Some("#alias:example.com"));

        storage.set_canonical_alias(&room_id, None).await.expect("set_canonical_alias None should succeed");
        let room = storage.get_room(&room_id).await.unwrap().unwrap();
        assert!(room.canonical_alias.is_none());

        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_update_canonical_alias() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let room_id = format!("!upd_alias_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_room(&room_id).await;
        storage.create_room(&room_id, "@c:example.com", "invite", "10", false).await.unwrap();

        storage
            .update_canonical_alias(&room_id, "#first:example.com")
            .await
            .expect("update_canonical_alias should succeed");
        let room = storage.get_room(&room_id).await.unwrap().unwrap();
        assert_eq!(room.canonical_alias.as_deref(), Some("#first:example.com"));

        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_get_room_alias_singular() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!single_alias_{}:example.com", suffix);
        let alias = format!("#single_{}:example.com", suffix);
        let _ = storage.delete_room(&room_id).await;
        storage.create_room(&room_id, "@c:example.com", "invite", "10", false).await.unwrap();
        storage.set_room_alias(&room_id, &alias, "@c:example.com").await.unwrap();

        // get_room_alias returns a single alias (LIMIT 1)
        let result = storage.get_room_alias(&room_id).await.expect("get_room_alias should succeed");
        assert_eq!(result.as_deref(), Some(alias.as_str()));

        // Absent room returns None
        let absent = storage.get_room_alias("!nonexistent:example.com").await.unwrap();
        assert!(absent.is_none());

        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_remove_room_alias_by_room_id() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!rm_alias_{}:example.com", suffix);
        let alias = format!("#rm_{}:example.com", suffix);
        let _ = storage.delete_room(&room_id).await;
        storage.create_room(&room_id, "@c:example.com", "invite", "10", false).await.unwrap();
        storage.set_room_alias(&room_id, &alias, "@c:example.com").await.unwrap();
        assert!(storage.get_room_by_alias(&alias).await.unwrap().is_some());

        storage.remove_room_alias(&room_id).await.expect("remove_room_alias should succeed");
        assert!(storage.get_room_by_alias(&alias).await.unwrap().is_none());

        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_set_room_version() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let room_id = format!("!version_test_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_room(&room_id).await;
        storage.create_room(&room_id, "@c:example.com", "invite", "10", false).await.unwrap();

        storage.set_room_version(&room_id, "11").await.expect("set_room_version should succeed");
        let room = storage.get_room(&room_id).await.unwrap().unwrap();
        assert_eq!(room.room_version, "11");

        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_shutdown_room_marks_private_and_renames() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let room_id = format!("!shutdown_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_room(&room_id).await;
        storage.create_room(&room_id, "@c:example.com", "invite", "10", true).await.unwrap();
        storage.set_room_directory(&room_id, true).await.unwrap();

        storage.shutdown_room(&room_id).await.expect("shutdown_room should succeed");
        let room = storage.get_room(&room_id).await.unwrap().unwrap();
        assert!(!room.is_public, "shutdown should make room private");
        assert!(room.name.as_deref().unwrap_or("").contains("SHUTDOWN"), "name should contain SHUTDOWN suffix");

        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_get_rooms_map_empty_input() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let map = storage.get_rooms_map(&[]).await.expect("get_rooms_map empty should succeed");
        assert!(map.is_empty());
    }

    #[tokio::test]
    async fn test_get_rooms_map_with_rooms() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let rid1 = format!("!map1_{}:example.com", uuid::Uuid::new_v4());
        let rid2 = format!("!map2_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_room(&rid1).await;
        let _ = storage.delete_room(&rid2).await;
        storage.create_room(&rid1, "@c:example.com", "invite", "10", false).await.unwrap();
        storage.create_room(&rid2, "@c:example.com", "invite", "10", false).await.unwrap();

        let map = storage.get_rooms_map(&[rid1.clone(), rid2.clone()]).await.expect("get_rooms_map should succeed");
        assert_eq!(map.len(), 2);
        assert!(map.contains_key(&rid1));
        assert!(map.contains_key(&rid2));

        let _ = storage.delete_room(&rid1).await;
        let _ = storage.delete_room(&rid2).await;
    }

    #[tokio::test]
    async fn test_get_rooms_batch_empty_input() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let rooms = storage.get_rooms_batch(&[]).await.expect("get_rooms_batch empty should succeed");
        assert!(rooms.is_empty());
    }

    #[tokio::test]
    async fn test_get_user_rooms_paginated_no_cursor() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        // Querying for a user with no memberships should return empty without error
        let rooms = storage
            .get_user_rooms_paginated("@paginated_nobody:example.com", 10, None)
            .await
            .expect("get_user_rooms_paginated no cursor should succeed");
        assert!(rooms.is_empty());
    }

    #[tokio::test]
    async fn test_get_user_room_list_summary_empty() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let summaries = storage
            .get_user_room_list_summary("@summary_nobody:example.com")
            .await
            .expect("get_user_room_list_summary should succeed");
        assert!(summaries.is_empty());
    }

    #[tokio::test]
    async fn test_set_room_account_data_upsert() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let room_id = format!("!acct_{}:example.com", uuid::Uuid::new_v4());
        let user_id = format!("@acct_user_{}:example.com", uuid::Uuid::new_v4());

        // Insert
        storage
            .set_room_account_data(&room_id, &user_id, "m.fully_read", &json!({"event_id": "$e1:example.com"}))
            .await
            .expect("set_room_account_data insert should succeed");

        // Upsert (same key, different content)
        storage
            .set_room_account_data(&room_id, &user_id, "m.fully_read", &json!({"event_id": "$e2:example.com"}))
            .await
            .expect("set_room_account_data upsert should succeed");

        // Cleanup
        sqlx::query("DELETE FROM room_account_data WHERE user_id = $1")
            .bind(&user_id)
            .execute(&*pool)
            .await
            .expect("cleanup room_account_data");
    }

    #[tokio::test]
    async fn test_update_read_marker_default_type() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let room_id = format!("!rm_{}:example.com", uuid::Uuid::new_v4());
        let user_id = format!("@rm_user_{}:example.com", uuid::Uuid::new_v4());

        storage
            .update_read_marker(&room_id, &user_id, "$event1:example.com")
            .await
            .expect("update_read_marker should succeed");

        // Default marker_type is m.fully_read
        let marker =
            storage.get_read_marker(&room_id, &user_id, "m.fully_read").await.expect("get_read_marker should succeed");
        assert_eq!(marker.as_deref(), Some("$event1:example.com"));

        sqlx::query("DELETE FROM read_markers WHERE user_id = $1").bind(&user_id).execute(&*pool).await.unwrap();
    }

    #[tokio::test]
    async fn test_update_read_marker_with_type() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let room_id = format!("!rmt_{}:example.com", uuid::Uuid::new_v4());
        let user_id = format!("@rmt_user_{}:example.com", uuid::Uuid::new_v4());

        storage
            .update_read_marker_with_type(&room_id, &user_id, "$e1:example.com", "m.read")
            .await
            .expect("update_read_marker_with_type m.read should succeed");
        storage
            .update_read_marker_with_type(&room_id, &user_id, "$e2:example.com", "m.marked_unread")
            .await
            .expect("update_read_marker_with_type m.marked_unread should succeed");

        let read = storage.get_read_marker(&room_id, &user_id, "m.read").await.unwrap();
        assert_eq!(read.as_deref(), Some("$e1:example.com"));
        let unread = storage.get_read_marker(&room_id, &user_id, "m.marked_unread").await.unwrap();
        assert_eq!(unread.as_deref(), Some("$e2:example.com"));

        sqlx::query("DELETE FROM read_markers WHERE user_id = $1").bind(&user_id).execute(&*pool).await.unwrap();
    }

    #[tokio::test]
    async fn test_get_read_marker_absent() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let marker = storage
            .get_read_marker("!nope:example.com", "@nobody:example.com", "m.fully_read")
            .await
            .expect("get_read_marker absent should succeed");
        assert!(marker.is_none());
    }

    #[tokio::test]
    async fn test_get_all_read_markers() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let room_id = format!("!allrm_{}:example.com", uuid::Uuid::new_v4());
        let user_id = format!("@allrm_user_{}:example.com", uuid::Uuid::new_v4());

        storage.update_read_marker_with_type(&room_id, &user_id, "$e1:example.com", "m.fully_read").await.unwrap();
        storage.update_read_marker_with_type(&room_id, &user_id, "$e2:example.com", "m.read").await.unwrap();

        let all = storage.get_all_read_markers(&room_id, &user_id).await.expect("get_all_read_markers should succeed");
        assert_eq!(all.len(), 2);
        assert_eq!(all.get("m.fully_read"), Some(&"$e1:example.com".to_string()));
        assert_eq!(all.get("m.read"), Some(&"$e2:example.com".to_string()));

        sqlx::query("DELETE FROM read_markers WHERE user_id = $1").bind(&user_id).execute(&*pool).await.unwrap();
    }

    #[tokio::test]
    async fn test_get_all_read_markers_empty() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let all = storage
            .get_all_read_markers("!nope:example.com", "@nobody:example.com")
            .await
            .expect("get_all_read_markers empty should succeed");
        assert!(all.is_empty());
    }

    #[tokio::test]
    async fn test_add_receipt_and_get_receipts() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let room_id = format!("!rcpt_{}:example.com", uuid::Uuid::new_v4());
        let user_id = format!("@rcpt_user_{}:example.com", uuid::Uuid::new_v4());
        let event_id = format!("$rcpt_evt_{}:example.com", uuid::Uuid::new_v4());

        storage
            .add_receipt(&user_id, "server", &room_id, &event_id, "m.read", &json!({"ts": 123}))
            .await
            .expect("add_receipt should succeed");

        let receipts = storage.get_receipts(&room_id, "m.read", &event_id).await.expect("get_receipts should succeed");
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].user_id, user_id);
        assert_eq!(receipts[0].event_id, event_id);
        assert_eq!(receipts[0].receipt_type, "m.read");

        sqlx::query("DELETE FROM event_receipts WHERE user_id = $1").bind(&user_id).execute(&*pool).await.unwrap();
    }

    #[tokio::test]
    async fn test_add_receipt_replaces_previous_for_same_type() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let room_id = format!("!rcpt2_{}:example.com", uuid::Uuid::new_v4());
        let user_id = format!("@rcpt2_user_{}:example.com", uuid::Uuid::new_v4());
        let event1 = format!("$evt1_{}:example.com", uuid::Uuid::new_v4());
        let event2 = format!("$evt2_{}:example.com", uuid::Uuid::new_v4());

        // Add first receipt
        storage.add_receipt(&user_id, "server", &room_id, &event1, "m.read", &json!({})).await.unwrap();
        // Add second receipt for the same user+type but different event:
        // add_receipt deletes prior receipts of the same type for other events
        storage.add_receipt(&user_id, "server", &room_id, &event2, "m.read", &json!({})).await.unwrap();

        // The first event should no longer have a receipt
        let receipts1 = storage.get_receipts(&room_id, "m.read", &event1).await.unwrap();
        assert!(receipts1.is_empty(), "old event receipt should be removed");

        // The new event should have the receipt
        let receipts2 = storage.get_receipts(&room_id, "m.read", &event2).await.unwrap();
        assert_eq!(receipts2.len(), 1);

        sqlx::query("DELETE FROM event_receipts WHERE user_id = $1").bind(&user_id).execute(&*pool).await.unwrap();
    }

    #[tokio::test]
    async fn test_get_receipts_empty() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let receipts = storage
            .get_receipts("!nope:example.com", "m.read", "$nope:example.com")
            .await
            .expect("get_receipts empty should succeed");
        assert!(receipts.is_empty());
    }

    #[tokio::test]
    async fn test_remove_room_directory() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let room_id = format!("!dirdel_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_room(&room_id).await;
        storage.create_room(&room_id, "@c:example.com", "invite", "10", true).await.unwrap();
        storage.set_room_directory(&room_id, true).await.unwrap();
        assert!(storage.is_room_in_directory(&room_id).await.unwrap());

        storage.remove_room_directory(&room_id).await.expect("remove_room_directory should succeed");
        assert!(!storage.is_room_in_directory(&room_id).await.unwrap());

        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_set_room_visibility_invalid_defaults_private() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        let room_id = format!("!vis_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_room(&room_id).await;
        storage.create_room(&room_id, "@c:example.com", "invite", "10", true).await.unwrap();
        assert!(storage.get_room(&room_id).await.unwrap().unwrap().is_public);

        // Unknown visibility string should default to "private"
        storage
            .set_room_visibility(&room_id, "unknown_value")
            .await
            .expect("set_room_visibility unknown should succeed");
        let room = storage.get_room(&room_id).await.unwrap().unwrap();
        assert!(!room.is_public, "unknown visibility should default to private");

        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_get_public_rooms_paginated_with_cursor() {
        let pool = test_pool().await;
        let storage = RoomStorage::new(&pool);
        // First page
        let first_page = storage.get_public_rooms_paginated(5, None, None).await.unwrap();
        assert!(first_page.len() <= 5);

        // If we got a full page, attempt a second page using the last item as cursor
        if let Some(last) = first_page.last() {
            let second_page = storage
                .get_public_rooms_paginated(5, Some(last.created_ts), Some(&last.room_id))
                .await
                .expect("get_public_rooms_paginated with cursor should succeed");
            // Second page should not overlap with first page's last item
            if let Some(second_last) = second_page.last() {
                assert!(second_last.room_id != last.room_id || second_last.created_ts != last.created_ts);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encryption_from_is_encrypted_true() {
        let result = RoomStorage::encryption_from_is_encrypted(Some(true));
        assert_eq!(result, Some("m.megolm.v1.aes-sha2".to_string()));
    }

    #[test]
    fn encryption_from_is_encrypted_false() {
        let result = RoomStorage::encryption_from_is_encrypted(Some(false));
        assert_eq!(result, None);
    }

    #[test]
    fn encryption_from_is_encrypted_none() {
        let result = RoomStorage::encryption_from_is_encrypted(None);
        assert_eq!(result, None);
    }
}

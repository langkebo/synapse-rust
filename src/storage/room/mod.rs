pub(crate) mod models;
pub mod admin;
pub use models::*;

use serde_json::json;
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use tracing;

use crate::common::room_versions::DEFAULT_ROOM_VERSION;

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
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query!(
            r"
            INSERT INTO rooms (room_id, creator, join_rules, room_version, is_public, history_visibility, created_ts, last_activity_ts)
            VALUES ($1, $2, $3, $4, $5, 'joined', $6, $6)
            ",
            room_id,
            creator,
            join_rule,
            version,
            is_public,
            now,
        )
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
        let row = sqlx::query_as!(
            RoomRecord,
            r"
            SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias,
                  r.join_rules AS join_rule, r.creator, r.room_version,
                  COALESCE(rs.member_count, joined.joined_members, 0) as member_count,
                  rs.is_encrypted, r.is_public, r.history_visibility, r.created_ts
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
            room_id,
        )
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

        let rows: Vec<RoomRecord> = sqlx::query_as::<_, RoomRecord>(
            r"
            SELECT
                r.room_id,
                r.name,
                r.topic,
                r.avatar_url,
                r.canonical_alias,
                r.join_rules AS join_rule,
                r.creator,
                r.room_version,
                r.is_public,
                rs.member_count,
                rs.is_encrypted,
                r.history_visibility,
                r.created_ts
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
        let result = sqlx::query_scalar!(
            r#"
            SELECT creator FROM rooms WHERE room_id = $1
            "#,
            room_id,
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.flatten())
    }

    pub async fn room_exists(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r#"
            SELECT 1 AS "exists!" FROM rooms WHERE room_id = $1 LIMIT 1
            "#,
            room_id,
        )
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
            sqlx::query_as!(
                RoomRecord,
                r"
                SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias,
                      r.join_rules AS join_rule, r.creator, r.room_version,
                      r.is_public, rs.member_count, rs.is_encrypted, r.history_visibility, r.created_ts
                FROM rooms r
                LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
                WHERE r.is_public = TRUE AND (r.created_ts < $2 OR (r.created_ts = $2 AND r.room_id < $3))
                ORDER BY r.created_ts DESC, r.room_id DESC
                LIMIT $1
                ",
                limit,
                ts,
                room_id,
            )
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query_as!(
                RoomRecord,
                r"
                SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias,
                      r.join_rules AS join_rule, r.creator, r.room_version,
                      r.is_public, rs.member_count, rs.is_encrypted, r.history_visibility, r.created_ts
                FROM rooms r
                LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
                WHERE r.is_public = TRUE
                ORDER BY r.created_ts DESC, r.room_id DESC
                LIMIT $1
                ",
                limit,
            )
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
        let count = sqlx::query_scalar!(
            r"
            SELECT COUNT(*) FROM rooms WHERE is_public = TRUE
            ",
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(count.unwrap_or(0))
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
        let rows: Vec<String> = sqlx::query_scalar!(
            r"
            SELECT room_id FROM room_memberships WHERE user_id = $1 AND membership = 'join'
            LIMIT 1000
            ",
            user_id,
        )
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

    async fn update_room_name_with_executor<'a, E>(executor: E, room_id: &str, name: &str) -> Result<(), sqlx::Error>
    where
        E: sqlx::Executor<'a, Database = Postgres>,
    {
        sqlx::query!(
            r"
            UPDATE rooms SET name = $1 WHERE room_id = $2
            ",
            name,
            room_id,
        )
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
        sqlx::query!(
            r"
            UPDATE rooms SET topic = $1 WHERE room_id = $2
            ",
            topic,
            room_id,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn update_room_avatar(&self, room_id: &str, avatar_url: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            UPDATE rooms SET avatar_url = $1 WHERE room_id = $2
            ",
            avatar_url,
            room_id,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_canonical_alias(&self, room_id: &str, alias: Option<&str>) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            UPDATE rooms SET canonical_alias = $1 WHERE room_id = $2
            ",
            alias,
            room_id,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_canonical_alias(&self, room_id: &str, alias: &str) -> Result<(), sqlx::Error> {
        self.set_canonical_alias(room_id, Some(alias)).await
    }

    pub async fn increment_member_count(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            UPDATE room_summaries
            SET member_count = member_count + 1,
                joined_member_count = joined_member_count + 1,
                updated_ts = $2
            WHERE room_id = $1
            ",
            room_id,
            chrono::Utc::now().timestamp_millis(),
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn decrement_member_count(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            UPDATE room_summaries
            SET member_count = GREATEST(member_count - 1, 0),
                joined_member_count = GREATEST(joined_member_count - 1, 0),
                updated_ts = $2
            WHERE room_id = $1
            ",
            room_id,
            chrono::Utc::now().timestamp_millis(),
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_room_count(&self) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar!(
            r"
            SELECT COALESCE(COUNT(*), 0) FROM rooms
            ",
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(count.unwrap_or(0))
    }

    pub async fn set_room_visibility(&self, room_id: &str, visibility: &str) -> Result<(), sqlx::Error> {
        let visibility_value = match visibility {
            "public" => "public",
            "private" => "private",
            _ => "private",
        };
        sqlx::query!(
            r"
            UPDATE rooms SET visibility = $1 WHERE room_id = $2
            ",
            visibility_value,
            room_id,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_room_alias(&self, room_id: &str, alias: &str, _created_by: &str) -> Result<(), sqlx::Error> {
        let creation_ts = chrono::Utc::now().timestamp_millis();
        let server_name = alias
            .rsplit_once(':')
            .map(|(_, server_name)| server_name)
            .filter(|server_name| !server_name.is_empty())
            .unwrap_or("localhost");
        sqlx::query!(
            r"
            INSERT INTO room_aliases (room_alias, room_id, server_name, created_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (room_alias) DO UPDATE SET
                room_id = EXCLUDED.room_id,
                created_ts = EXCLUDED.created_ts
            ",
            alias,
            room_id,
            server_name,
            creation_ts,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove_room_alias(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            DELETE FROM room_aliases WHERE room_id = $1
            ",
            room_id,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove_room_alias_by_name(&self, alias: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            DELETE FROM room_aliases WHERE room_alias = $1
            ",
            alias,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_room(&self, room_id: &str) -> Result<(), sqlx::Error> {
        tracing::info!(room_id = %room_id, "Deleting room");
        sqlx::query!(
            r"
            DELETE FROM rooms WHERE room_id = $1
            ",
            room_id,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn shutdown_room(&self, room_id: &str) -> Result<(), sqlx::Error> {
        tracing::info!(room_id = %room_id, "Shutting down room");
        // Mark room as inactive or delete it. For simplicity, we delete it from directory
        // and mark its name to indicate it's shutdown.
        sqlx::query!(
            "UPDATE rooms SET is_public = false, name = COALESCE(name, '') || ' (SHUTDOWN)' WHERE room_id = $1",
            room_id,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_room_version(&self, room_id: &str, version: &str) -> Result<(), sqlx::Error> {
        sqlx::query!("UPDATE rooms SET room_version = $1 WHERE room_id = $2",
            version,
            room_id,
        )
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    pub async fn copy_room_state(&self, source_room_id: &str, target_room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            INSERT INTO room_state_events (room_id, type, state_key, content, sender, origin_server_ts)
            SELECT $1, event_type, state_key, content, sender, origin_server_ts
            FROM (
                SELECT DISTINCT ON (event_type, state_key)
                    event_type, state_key, content, sender, origin_server_ts
                FROM events
                WHERE room_id = $2 AND state_key IS NOT NULL
                ORDER BY event_type, state_key, origin_server_ts DESC
            ) sub
            ON CONFLICT (room_id, type, state_key) DO UPDATE SET
                content = EXCLUDED.content,
                sender = EXCLUDED.sender,
                origin_server_ts = EXCLUDED.origin_server_ts
            ",
            target_room_id,
            source_room_id,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_room_alias(&self, room_id: &str) -> Result<Option<String>, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r"
            SELECT room_alias FROM room_aliases WHERE room_id = $1 LIMIT 1
            ",
            room_id,
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result)
    }

    pub async fn get_room_aliases(&self, room_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let results = sqlx::query_scalar!(
            r"
            SELECT room_alias FROM room_aliases WHERE room_id = $1
            ",
            room_id,
        )
        .fetch_all(&*self.pool)
        .await?;
        Ok(results)
    }

    pub async fn get_room_by_alias(&self, alias: &str) -> Result<Option<String>, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r"
            SELECT room_id FROM room_aliases WHERE room_alias = $1
            ",
            alias,
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result)
    }

    pub async fn is_room_in_directory(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r#"
            SELECT COALESCE(is_public, FALSE) AS "is_public!" FROM room_directory WHERE room_id = $1
            "#,
            room_id,
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.unwrap_or(false))
    }

    pub async fn set_room_directory(&self, room_id: &str, is_public: bool) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query!(
            r"
            INSERT INTO room_directory (room_id, is_public, added_ts)
            VALUES ($1, $2, $3)
            ON CONFLICT (room_id) DO UPDATE SET is_public = EXCLUDED.is_public
            ",
            room_id,
            is_public,
            now,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query!(
            r"
            UPDATE rooms SET is_public = $1 WHERE room_id = $2
            ",
            is_public,
            room_id,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove_room_directory(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            DELETE FROM room_directory WHERE room_id = $1
            ",
            room_id,
        )
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
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query!(
            r"
            INSERT INTO room_account_data (user_id, room_id, data_type, data, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $5)
            ON CONFLICT (user_id, room_id, data_type) DO UPDATE SET data = EXCLUDED.data, updated_ts = EXCLUDED.updated_ts
            ",
            user_id,
            room_id,
            event_type,
            content,
            now,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_read_marker(&self, room_id: &str, user_id: &str, event_id: &str) -> Result<(), sqlx::Error> {
        let now: i64 = chrono::Utc::now().timestamp_millis();
        sqlx::query!(
            r"
            INSERT INTO read_markers (room_id, user_id, event_id, marker_type, created_ts, updated_ts)
            VALUES ($1, $2, $3, 'm.fully_read', $4, $4)
            ON CONFLICT (room_id, user_id, marker_type) DO UPDATE SET event_id = EXCLUDED.event_id, updated_ts = EXCLUDED.updated_ts
            ",
            room_id,
            user_id,
            event_id,
            now,
        )
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
        sqlx::query!(
            r"
            INSERT INTO read_markers (room_id, user_id, event_id, marker_type, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $5)
            ON CONFLICT (room_id, user_id, marker_type) DO UPDATE SET event_id = EXCLUDED.event_id, updated_ts = EXCLUDED.updated_ts
            ",
            room_id,
            user_id,
            event_id,
            marker_type,
            now,
        )
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
        let result = sqlx::query_scalar!(
            r"
            SELECT event_id FROM read_markers
            WHERE room_id = $1 AND user_id = $2 AND marker_type = $3
            ",
            room_id,
            user_id,
            marker_type,
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result)
    }

    /// Get all read markers for a user in a room
    pub async fn get_all_read_markers(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<std::collections::HashMap<String, String>, sqlx::Error> {
        let rows = sqlx::query!(
            r"
            SELECT marker_type, event_id FROM read_markers
            WHERE room_id = $1 AND user_id = $2
            ",
            room_id,
            user_id,
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| (r.marker_type, r.event_id)).collect())
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
        let receipt_data = if data.is_object() { data.clone() } else { json!({}) };

        sqlx::query!(
            r"
            DELETE FROM event_receipts
            WHERE room_id = $1
              AND user_id = $2
              AND receipt_type = $3
              AND event_id <> $4
            ",
            room_id,
            user_id,
            receipt_type,
            event_id,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query!(
            r"
            INSERT INTO event_receipts (event_id, room_id, user_id, receipt_type, ts, data, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $5, $5)
            ON CONFLICT (event_id, room_id, user_id, receipt_type) DO UPDATE
            SET ts = EXCLUDED.ts, data = EXCLUDED.data, updated_ts = EXCLUDED.updated_ts
            ",
            event_id,
            room_id,
            user_id,
            receipt_type,
            now,
            &receipt_data,
        )
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
        let rows = sqlx::query!(
            r#"
            SELECT user_id, event_id, receipt_type, ts, COALESCE(data, '{}') AS "data!" FROM event_receipts
            WHERE room_id = $1 AND receipt_type = $2 AND event_id = $3
            "#,
            room_id,
            receipt_type,
            event_id,
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| Receipt {
                user_id: row.user_id,
                event_id: row.event_id,
                receipt_type: row.receipt_type,
                ts: row.ts,
                data: row.data,
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

        let rows: Vec<RoomWithMembersRecord> = sqlx::query_as!(
            RoomWithMembersRecord,
            r"
            SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias,
                   r.join_rules AS join_rule, r.creator, r.room_version, r.is_public,
                   rs.member_count, rs.is_encrypted,
                   r.history_visibility, r.created_ts,
                   COUNT(rm.user_id) as joined_members
            FROM rooms r
            LEFT JOIN room_memberships rm ON r.room_id = rm.room_id AND rm.membership = 'join'
            LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
            WHERE r.room_id = ANY($1)
            GROUP BY r.room_id, rs.member_count, rs.is_encrypted
            ",
            room_ids,
        )
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

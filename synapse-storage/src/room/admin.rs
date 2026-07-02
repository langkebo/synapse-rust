use super::models::*;
use serde_json::json;
use tracing;

use synapse_common::room_versions::DEFAULT_ROOM_VERSION;

impl RoomStorage {
    pub async fn check_rooms_exist_batch(
        &self,
        room_ids: &[String],
    ) -> Result<std::collections::HashSet<String>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashSet::new());
        }

        let rows: Vec<String> = sqlx::query_scalar(
            r"
            SELECT room_id FROM rooms WHERE room_id = ANY($1)
            ",
        )
        .bind(room_ids)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().collect())
    }

    /// Clean up abnormal data (orphan data, empty rooms, etc.)
    ///
    /// # Arguments
    ///
    /// * `min_age_ms` - Minimum room lifetime (milliseconds); empty rooms younger than this will not be cleaned up. Default 24h.
    pub async fn cleanup_abnormal_data(&self, min_age_ms: Option<i64>) -> Result<serde_json::Value, sqlx::Error> {
        tracing::info!(min_age_ms = min_age_ms, "Starting abnormal data cleanup");
        let min_age = min_age_ms.unwrap_or(24 * 60 * 60 * 1000);
        let now = chrono::Utc::now().timestamp_millis();
        let cutoff = now - min_age;

        let mut results = serde_json::Map::new();

        // 1. Clean up rooms with no members and older than min_age
        let deleted_empty_rooms = sqlx::query(
            r"
            DELETE FROM rooms
            WHERE created_ts < $1
            AND NOT EXISTS (
                SELECT 1 FROM room_memberships
                WHERE room_memberships.room_id = rooms.room_id
                AND membership = 'join'
            )
            ",
        )
        .bind(cutoff)
        .execute(&*self.pool)
        .await?
        .rows_affected();
        results.insert("deleted_empty_rooms".to_string(), json!(deleted_empty_rooms));

        // 2. Clean up orphan events (events pointing to non-existent rooms)
        let deleted_orphan_events = sqlx::query(
            r"
            DELETE FROM events
            WHERE NOT EXISTS (SELECT 1 FROM rooms WHERE rooms.room_id = events.room_id)
            ",
        )
        .execute(&*self.pool)
        .await?
        .rows_affected();
        results.insert("deleted_orphan_events".to_string(), json!(deleted_orphan_events));

        // 3. Clean up orphan memberships
        let deleted_orphan_memberships = sqlx::query(
            r"
            DELETE FROM room_memberships
            WHERE NOT EXISTS (SELECT 1 FROM rooms WHERE rooms.room_id = room_memberships.room_id)
            ",
        )
        .execute(&*self.pool)
        .await?
        .rows_affected();
        results.insert("deleted_orphan_memberships".to_string(), json!(deleted_orphan_memberships));

        // 4. Clean up orphan state
        let deleted_orphan_state = sqlx::query(
            r"
            DELETE FROM room_state_events
            WHERE NOT EXISTS (SELECT 1 FROM rooms WHERE rooms.room_id = room_state_events.room_id)
            ",
        )
        .execute(&*self.pool)
        .await?
        .rows_affected();
        results.insert("deleted_orphan_state".to_string(), json!(deleted_orphan_state));

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
                r"
                SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias, r.join_rules, r.creator, r.room_version,
                      r.is_public, rs.member_count as member_count, rs.is_encrypted as is_encrypted, r.history_visibility, r.created_ts
                FROM rooms r
                LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
                WHERE r.is_public = TRUE
                  AND (r.created_ts < $2 OR (r.created_ts = $2 AND r.room_id < $3))
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
            r"
            SELECT room_id, room_alias FROM room_aliases WHERE room_id = ANY($1)
            ",
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
            r"
            SELECT room_alias, room_id FROM room_aliases WHERE room_alias = ANY($1)
            ",
        )
        .bind(aliases)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().collect())
    }

    pub async fn increment_member_counts_batch(&self, room_ids: &[String]) -> Result<u64, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(0);
        }

        let result = sqlx::query(
            r"
            UPDATE room_summaries
            SET member_count = member_count + 1,
                joined_member_count = joined_member_count + 1,
                updated_ts = $2
            WHERE room_id = ANY($1)
            ",
        )
        .bind(room_ids)
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn decrement_member_counts_batch(&self, room_ids: &[String]) -> Result<u64, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(0);
        }

        let result = sqlx::query(
            r"
            UPDATE room_summaries
            SET member_count = GREATEST(member_count - 1, 0),
                joined_member_count = GREATEST(joined_member_count - 1, 0),
                updated_ts = $2
            WHERE room_id = ANY($1)
            ",
        )
        .bind(room_ids)
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn block_room(
        &self,
        room_id: &str,
        blocked_at: i64,
        blocked_by: &str,
        reason: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            INSERT INTO blocked_rooms (room_id, blocked_at, blocked_by, reason)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (room_id) DO UPDATE SET blocked_at = $2, reason = $4
            ",
        )
        .bind(room_id)
        .bind(blocked_at)
        .bind(blocked_by)
        .bind(reason)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_room_block_status(&self, room_id: &str) -> Result<Option<i64>, sqlx::Error> {
        let result: Option<(i64,)> = sqlx::query_as(r"SELECT blocked_at FROM blocked_rooms WHERE room_id = $1")
            .bind(room_id)
            .fetch_optional(&*self.pool)
            .await?;
        Ok(result.map(|r| r.0))
    }

    pub async fn unblock_room(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(r"DELETE FROM blocked_rooms WHERE room_id = $1").bind(room_id).execute(&*self.pool).await?;
        Ok(())
    }

    pub async fn get_room_stats_overview(&self) -> Result<serde_json::Value, sqlx::Error> {
        let total_rooms: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rooms").fetch_one(&*self.pool).await?;

        let encrypted_rooms: i64 = sqlx::query_scalar(
            r"SELECT COUNT(DISTINCT room_id) FROM events WHERE event_type = 'm.room.encryption' AND state_key IS NOT NULL AND room_id IN (SELECT room_id FROM rooms)",
        )
        .fetch_one(&*self.pool)
        .await?;

        let public_rooms: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM rooms WHERE is_public = true").fetch_one(&*self.pool).await?;

        let total_messages: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_type = 'm.room.message'")
            .fetch_one(&*self.pool)
            .await?;

        let total_members: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM room_memberships").fetch_one(&*self.pool).await?;

        let active_rooms: i64 =
            sqlx::query_scalar("SELECT COUNT(DISTINCT room_id) FROM events WHERE origin_server_ts > $1")
                .bind(chrono::Utc::now().timestamp_millis() - 7 * 24 * 60 * 60 * 1000)
                .fetch_one(&*self.pool)
                .await?;

        Ok(json!({
            "total_rooms": total_rooms,
            "encrypted_rooms": encrypted_rooms,
            "public_rooms": public_rooms,
            "total_messages": total_messages,
            "total_members": total_members,
            "active_rooms": active_rooms,
            "average_messages_per_room": if total_rooms > 0 { total_messages / total_rooms } else { 0 }
        }))
    }

    pub async fn get_single_room_stats(&self, room_id: &str) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let room_exists: Option<(String,)> = sqlx::query_as(r"SELECT room_id FROM rooms WHERE room_id = $1")
            .bind(room_id)
            .fetch_optional(&*self.pool)
            .await?;

        if room_exists.is_none() {
            return Ok(None);
        }

        let member_count: i64 =
            sqlx::query_scalar(r"SELECT COUNT(*) FROM room_memberships WHERE room_id = $1 AND membership = 'join'")
                .bind(room_id)
                .fetch_one(&*self.pool)
                .await?;

        let message_count: i64 =
            sqlx::query_scalar(r"SELECT COUNT(*) FROM events WHERE room_id = $1 AND event_type = 'm.room.message'")
                .bind(room_id)
                .fetch_one(&*self.pool)
                .await?;

        let last_message_ts: Option<i64> =
            sqlx::query_scalar("SELECT MAX(origin_server_ts) FROM events WHERE room_id = $1")
                .bind(room_id)
                .fetch_optional(&*self.pool)
                .await?
                .flatten();

        let is_encrypted: bool = sqlx::query_scalar(
            r"SELECT EXISTS(SELECT 1 FROM events WHERE room_id = $1 AND event_type = 'm.room.encryption' AND state_key IS NOT NULL)",
        )
        .bind(room_id)
        .fetch_one(&*self.pool)
        .await?;

        let admin_count: i64 = sqlx::query_scalar(
            r"SELECT COUNT(*) FROM room_memberships WHERE room_id = $1 AND membership = 'join' AND user_id IN (SELECT user_id FROM users WHERE is_admin = true)",
        )
        .bind(room_id)
        .fetch_one(&*self.pool)
        .await?;

        Ok(Some(json!({
            "room_id": room_id,
            "member_count": member_count,
            "message_count": message_count,
            "last_message_ts": last_message_ts,
            "is_encrypted": is_encrypted,
            "admin_count": admin_count
        })))
    }

    pub async fn get_room_listings_status(&self, room_id: &str) -> Result<Option<(bool, bool)>, sqlx::Error> {
        let is_public: Option<bool> = sqlx::query_scalar("SELECT is_public FROM rooms WHERE room_id = $1")
            .bind(room_id)
            .fetch_optional(&*self.pool)
            .await?;

        let Some(is_public) = is_public else {
            return Ok(None);
        };

        let in_directory: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM room_directory WHERE room_id = $1)")
            .bind(room_id)
            .fetch_one(&*self.pool)
            .await?;

        Ok(Some((is_public, in_directory)))
    }

    pub async fn set_room_public_with_directory(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("UPDATE rooms SET is_public = true WHERE room_id = $1")
            .bind(room_id)
            .execute(&*self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Ok(false);
        }

        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r"INSERT INTO room_directory (room_id, is_public, added_ts) VALUES ($1, true, $2) ON CONFLICT (room_id) DO UPDATE SET is_public = true",
        )
        .bind(room_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(true)
    }

    pub async fn set_room_private_with_directory(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("UPDATE rooms SET is_public = false WHERE room_id = $1")
            .bind(room_id)
            .execute(&*self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Ok(false);
        }

        sqlx::query("DELETE FROM room_directory WHERE room_id = $1").bind(room_id).execute(&*self.pool).await?;

        Ok(true)
    }

    pub async fn get_room_version_only(&self, room_id: &str) -> Result<Option<String>, sqlx::Error> {
        let result: Option<(String,)> = sqlx::query_as(r"SELECT room_version FROM rooms WHERE room_id = $1")
            .bind(room_id)
            .fetch_optional(&*self.pool)
            .await?;
        Ok(result.map(|r| r.0))
    }

    pub async fn search_all_rooms_admin(
        &self,
        search_term: Option<&str>,
        limit: i64,
        order_by: RoomSearchOrder,
        cursor: Option<RoomSearchCursor>,
        is_public: Option<bool>,
        is_encrypted: Option<bool>,
    ) -> Result<(Vec<serde_json::Value>, i64, Option<String>), sqlx::Error> {
        let search_pattern = search_term.map(|term| format!("%{term}%"));
        let search_term_owned = search_term.map(|term| term.to_string());

        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            r"
            SELECT r.room_id, r.name, r.topic, r.creator, r.is_public, r.created_ts as creation_ts,
                   COUNT(DISTINCT rm.user_id) as member_count,
                   CASE WHEN COUNT(DISTINCT e.event_id) > 0 THEN TRUE ELSE FALSE END as is_encrypted
            FROM rooms r
            LEFT JOIN room_memberships rm ON r.room_id = rm.room_id AND rm.membership = 'join'
            LEFT JOIN events e ON r.room_id = e.room_id AND e.event_type = 'm.room.encryption' AND e.state_key IS NOT NULL
            WHERE 1=1
            ",
        );

        if let Some(pattern) = &search_pattern {
            if let Some(term) = &search_term_owned {
                if term.len() >= 3 {
                    query.push(" AND (r.name ILIKE ");
                    query.push_bind(pattern);
                    query.push(" OR r.name % ");
                    query.push_bind(term);
                    query.push(" OR r.topic ILIKE ");
                    query.push_bind(pattern);
                    query.push(" OR r.canonical_alias ILIKE ");
                    query.push_bind(pattern);
                    query.push(" OR r.canonical_alias % ");
                    query.push_bind(term);
                    query.push(" OR r.room_id ILIKE ");
                    query.push_bind(pattern);
                    query.push(")");
                } else {
                    query.push(" AND (r.name ILIKE ");
                    query.push_bind(pattern);
                    query.push(" OR r.topic ILIKE ");
                    query.push_bind(pattern);
                    query.push(" OR r.canonical_alias ILIKE ");
                    query.push_bind(pattern);
                    query.push(" OR r.room_id ILIKE ");
                    query.push_bind(pattern);
                    query.push(")");
                }
            } else {
                query.push(" AND (r.name ILIKE ");
                query.push_bind(pattern);
                query.push(" OR r.topic ILIKE ");
                query.push_bind(pattern);
                query.push(" OR r.room_id ILIKE ");
                query.push_bind(pattern);
                query.push(")");
            }
        }

        if let Some(is_pub) = is_public {
            query.push(" AND r.is_public = ");
            query.push_bind(is_pub);
        }

        if let Some(is_enc) = is_encrypted {
            if is_enc {
                query.push(
                    " AND EXISTS (SELECT 1 FROM events encryption_events WHERE encryption_events.room_id = r.room_id AND encryption_events.event_type = 'm.room.encryption' AND encryption_events.state_key IS NOT NULL)",
                );
            } else {
                query.push(
                    " AND NOT EXISTS (SELECT 1 FROM events encryption_events WHERE encryption_events.room_id = r.room_id AND encryption_events.event_type = 'm.room.encryption' AND encryption_events.state_key IS NOT NULL)",
                );
            }
        }

        query.push(" GROUP BY r.room_id, r.name, r.topic, r.creator, r.is_public, r.created_ts");

        match &cursor {
            Some(RoomSearchCursor::Created { created_ts, room_id }) => {
                query.push(" HAVING (r.created_ts < ");
                query.push_bind(*created_ts);
                query.push(" OR (r.created_ts = ");
                query.push_bind(*created_ts);
                query.push(" AND r.room_id < ");
                query.push_bind(room_id);
                query.push("))");
            }
            Some(RoomSearchCursor::Name { name, created_ts, room_id }) => {
                match name {
                    Some(name) => {
                        query.push(" HAVING (r.name IS NULL OR r.name > ");
                        query.push_bind(name);
                        query.push(" OR (r.name = ");
                        query.push_bind(name);
                        query.push(" AND (r.created_ts < ");
                        query.push_bind(*created_ts);
                        query.push(" OR (r.created_ts = ");
                        query.push_bind(*created_ts);
                        query.push(" AND r.room_id < ");
                        query.push_bind(room_id);
                        query.push("))))");
                    }
                    None => {
                        query.push(" HAVING r.name IS NULL AND (r.created_ts < ");
                        query.push_bind(*created_ts);
                        query.push(" OR (r.created_ts = ");
                        query.push_bind(*created_ts);
                        query.push(" AND r.room_id < ");
                        query.push_bind(room_id);
                        query.push("))");
                    }
                };
            }
            Some(RoomSearchCursor::Size { member_count, created_ts, room_id }) => {
                query.push(" HAVING (COUNT(DISTINCT rm.user_id) < ");
                query.push_bind(*member_count);
                query.push(" OR (COUNT(DISTINCT rm.user_id) = ");
                query.push_bind(*member_count);
                query.push(" AND r.created_ts < ");
                query.push_bind(*created_ts);
                query.push(") OR (COUNT(DISTINCT rm.user_id) = ");
                query.push_bind(*member_count);
                query.push(" AND r.created_ts = ");
                query.push_bind(*created_ts);
                query.push(" AND r.room_id < ");
                query.push_bind(room_id);
                query.push("))");
            }
            None => {}
        }

        let order_by_clause = match order_by {
            RoomSearchOrder::Name => " ORDER BY r.name ASC NULLS LAST, r.created_ts DESC, r.room_id DESC",
            RoomSearchOrder::Size => " ORDER BY member_count DESC, r.created_ts DESC, r.room_id DESC",
            RoomSearchOrder::Created => " ORDER BY r.created_ts DESC, r.room_id DESC",
        };
        query.push(order_by_clause);

        query.push(" LIMIT ");
        query.push_bind(limit);

        let rooms = query.build().fetch_all(&*self.pool).await?;

        let mut count_query =
            sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT COUNT(*) as total FROM rooms r WHERE 1=1");

        if let Some(pattern) = &search_pattern {
            count_query.push(" AND (r.name ILIKE ");
            count_query.push_bind(pattern);
            count_query.push(" OR r.topic ILIKE ");
            count_query.push_bind(pattern);
            count_query.push(" OR r.room_id ILIKE ");
            count_query.push_bind(pattern);
            count_query.push(")");
        }

        if let Some(is_pub) = is_public {
            count_query.push(" AND r.is_public = ");
            count_query.push_bind(is_pub);
        }

        if let Some(is_enc) = is_encrypted {
            if is_enc {
                count_query.push(
                    " AND EXISTS (SELECT 1 FROM events encryption_events WHERE encryption_events.room_id = r.room_id AND encryption_events.event_type = 'm.room.encryption' AND encryption_events.state_key IS NOT NULL)",
                );
            } else {
                count_query.push(
                    " AND NOT EXISTS (SELECT 1 FROM events encryption_events WHERE encryption_events.room_id = r.room_id AND encryption_events.event_type = 'm.room.encryption' AND encryption_events.state_key IS NOT NULL)",
                );
            }
        }

        let total_row = count_query.build().fetch_one(&*self.pool).await?;
        let total: i64 = total_row.get("total");

        use sqlx::Row;
        let results: Vec<serde_json::Value> = rooms
            .iter()
            .map(|r| {
                json!({
                    "room_id": r.get::<String, _>("room_id"),
                    "name": r.get::<Option<String>, _>("name"),
                    "topic": r.get::<Option<String>, _>("topic"),
                    "creator": r.get::<Option<String>, _>("creator"),
                    "is_public": r.get::<bool, _>("is_public"),
                    "member_count": r.get::<i64, _>("member_count"),
                    "is_encrypted": r.get::<bool, _>("is_encrypted"),
                    "creation_ts": r.get::<i64, _>("creation_ts")
                })
            })
            .collect();

        let next_batch = if rooms.len() as i64 == limit {
            rooms.last().map(|r| match order_by {
                RoomSearchOrder::Created => encode_room_search_cursor(&RoomSearchCursor::Created {
                    created_ts: r.get::<i64, _>("creation_ts"),
                    room_id: r.get::<String, _>("room_id"),
                }),
                RoomSearchOrder::Name => encode_room_search_cursor(&RoomSearchCursor::Name {
                    name: r.get::<Option<String>, _>("name"),
                    created_ts: r.get::<i64, _>("creation_ts"),
                    room_id: r.get::<String, _>("room_id"),
                }),
                RoomSearchOrder::Size => encode_room_search_cursor(&RoomSearchCursor::Size {
                    member_count: r.get::<i64, _>("member_count"),
                    created_ts: r.get::<i64, _>("creation_ts"),
                    room_id: r.get::<String, _>("room_id"),
                }),
            })
        } else {
            None
        };

        Ok((results, total, next_batch))
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

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::{Pool, Postgres};
    use std::sync::Arc;

    async fn test_pool() -> Arc<Pool<Postgres>> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    async fn ensure_test_room(pool: &Pool<Postgres>, room_id: &str) {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"INSERT INTO rooms (room_id, creator, join_rules, room_version, is_public, history_visibility, created_ts, last_activity_ts)
               VALUES ($1, '@test:example.com', 'invite', '10', false, 'joined', $2, $2)
               ON CONFLICT (room_id) DO NOTHING"#,
        )
        .bind(room_id)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test room");
    }

    #[tokio::test]
    async fn test_check_rooms_exist_batch() {
        let pool = test_pool().await;
        let storage = RoomStorage { pool: pool.clone() };
        let room_id = format!("!check_batch_{}:example.com", uuid::Uuid::new_v4());

        // Clean up at start
        let _ = storage.delete_room(&room_id).await;
        ensure_test_room(&pool, &room_id).await;

        let existing = storage.check_rooms_exist_batch(&[room_id.clone()]).await.expect("should succeed");
        assert!(existing.contains(&room_id), "batch should contain the room");

        // Clean up at end
        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_check_rooms_exist_batch_empty() {
        let pool = test_pool().await;
        let storage = RoomStorage { pool: pool.clone() };

        let existing = storage.check_rooms_exist_batch(&[]).await.expect("should succeed");
        assert!(existing.is_empty(), "empty input should return empty set");
    }

    #[tokio::test]
    async fn test_block_room_and_get_status() {
        let pool = test_pool().await;
        let storage = RoomStorage { pool: pool.clone() };
        let room_id = format!("!block_test_{}:example.com", uuid::Uuid::new_v4());

        // Clean up at start
        let _ = storage.delete_room(&room_id).await;
        ensure_test_room(&pool, &room_id).await;

        let blocked_at = chrono::Utc::now().timestamp_millis();
        storage
            .block_room(&room_id, blocked_at, "@admin:example.com", Some("spam"))
            .await
            .expect("block should succeed");

        let status = storage.get_room_block_status(&room_id).await.expect("get status should succeed");
        assert_eq!(status, Some(blocked_at));

        // Clean up at end
        let _ = storage.unblock_room(&room_id).await;
        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_get_room_block_status_not_blocked() {
        let pool = test_pool().await;
        let storage = RoomStorage { pool: pool.clone() };
        let room_id = format!("!notblocked_{}:example.com", uuid::Uuid::new_v4());

        // Clean up at start
        let _ = storage.delete_room(&room_id).await;
        ensure_test_room(&pool, &room_id).await;

        let status = storage.get_room_block_status(&room_id).await.expect("get status should succeed");
        assert!(status.is_none(), "unblocked room should return None");

        // Clean up at end
        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_unblock_room() {
        let pool = test_pool().await;
        let storage = RoomStorage { pool: pool.clone() };
        let room_id = format!("!unblock_test_{}:example.com", uuid::Uuid::new_v4());

        // Clean up at start
        let _ = storage.delete_room(&room_id).await;
        ensure_test_room(&pool, &room_id).await;

        let blocked_at = chrono::Utc::now().timestamp_millis();
        storage.block_room(&room_id, blocked_at, "@admin:example.com", None).await.expect("block should succeed");

        storage.unblock_room(&room_id).await.expect("unblock should succeed");

        let status = storage.get_room_block_status(&room_id).await.expect("get status should succeed");
        assert!(status.is_none(), "unblocked room should return None");

        // Clean up at end
        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_get_room_version_only_found() {
        let pool = test_pool().await;
        let storage = RoomStorage { pool: pool.clone() };
        let room_id = format!("!version_test_{}:example.com", uuid::Uuid::new_v4());

        // Clean up at start
        let _ = storage.delete_room(&room_id).await;
        ensure_test_room(&pool, &room_id).await;

        let version = storage.get_room_version_only(&room_id).await.expect("should succeed");
        assert_eq!(version, Some("10".to_string()));

        // Clean up at end
        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_get_room_version_only_not_found() {
        let pool = test_pool().await;
        let storage = RoomStorage { pool: pool.clone() };

        let version = storage.get_room_version_only("!nonexistent:example.com").await.expect("should succeed");
        assert!(version.is_none());
    }

    #[tokio::test]
    async fn test_set_room_public_with_directory() {
        let pool = test_pool().await;
        let storage = RoomStorage { pool: pool.clone() };
        let room_id = format!("!pubdir_test_{}:example.com", uuid::Uuid::new_v4());

        // Clean up at start
        let _ = storage.delete_room(&room_id).await;
        ensure_test_room(&pool, &room_id).await;

        let result = storage.set_room_public_with_directory(&room_id).await.expect("should succeed");
        assert!(result, "should return true on success");

        // Verify room is in directory
        let in_dir: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM room_directory WHERE room_id = $1)")
            .bind(&room_id)
            .fetch_one(&*pool)
            .await
            .expect("should query directory");
        assert!(in_dir, "room should be in directory");

        // Clean up at end
        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_set_room_private_with_directory() {
        let pool = test_pool().await;
        let storage = RoomStorage { pool: pool.clone() };
        let room_id = format!("!privdir_test_{}:example.com", uuid::Uuid::new_v4());

        // Clean up at start
        let _ = storage.delete_room(&room_id).await;
        ensure_test_room(&pool, &room_id).await;

        // First make it public
        storage.set_room_public_with_directory(&room_id).await.expect("set public should succeed");

        // Then make it private
        let result = storage.set_room_private_with_directory(&room_id).await.expect("should succeed");
        assert!(result, "should return true on success");

        // Verify room is NOT in directory
        let in_dir: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM room_directory WHERE room_id = $1)")
            .bind(&room_id)
            .fetch_one(&*pool)
            .await
            .expect("should query directory");
        assert!(!in_dir, "room should NOT be in directory");

        // Clean up at end
        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_get_room_listings_status() {
        let pool = test_pool().await;
        let storage = RoomStorage { pool: pool.clone() };
        let room_id = format!("!listing_test_{}:example.com", uuid::Uuid::new_v4());

        // Clean up at start
        let _ = storage.delete_room(&room_id).await;
        ensure_test_room(&pool, &room_id).await;

        let (is_public, in_directory) =
            storage.get_room_listings_status(&room_id).await.expect("should succeed").expect("room should exist");
        assert!(!is_public, "new room should not be public");
        assert!(!in_directory, "new room should not be in directory");

        // Clean up at end
        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_get_single_room_stats() {
        let pool = test_pool().await;
        let storage = RoomStorage { pool: pool.clone() };
        let room_id = format!("!stats_test_{}:example.com", uuid::Uuid::new_v4());

        // Clean up at start
        let _ = storage.delete_room(&room_id).await;
        ensure_test_room(&pool, &room_id).await;

        let stats = storage.get_single_room_stats(&room_id).await.expect("should succeed").expect("room should exist");
        assert_eq!(stats["room_id"].as_str().unwrap(), room_id);
        assert!(stats["member_count"].as_i64().is_some());
        assert!(stats["message_count"].as_i64().is_some());

        // Clean up at end
        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_get_room_stats_overview() {
        let pool = test_pool().await;
        let storage = RoomStorage { pool: pool.clone() };
        let room_id = format!("!overview_test_{}:example.com", uuid::Uuid::new_v4());

        // Clean up at start
        let _ = storage.delete_room(&room_id).await;
        ensure_test_room(&pool, &room_id).await;

        let overview = storage.get_room_stats_overview().await.expect("should succeed");
        assert!(overview["total_rooms"].as_i64().is_some());
        assert!(overview["public_rooms"].as_i64().is_some());
        assert!(overview["total_messages"].as_i64().is_some());
        assert!(overview["total_members"].as_i64().is_some());

        // Clean up at end
        let _ = storage.delete_room(&room_id).await;
    }

    #[tokio::test]
    async fn test_get_room_aliases_batch() {
        let pool = test_pool().await;
        let storage = RoomStorage { pool: pool.clone() };
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!aliasbt_{}:example.com", suffix);
        let alias = format!("#mytest_{}:example.com", suffix);

        // Clean up at start
        let _ = storage.delete_room(&room_id).await;
        ensure_test_room(&pool, &room_id).await;

        storage.set_room_alias(&room_id, &alias, "@test:example.com").await.expect("set alias should succeed");

        let aliases = storage.get_room_aliases_batch(&[room_id.clone()]).await.expect("should succeed");
        assert!(aliases.contains_key(&room_id), "should contain room key in batch result");
        assert!(aliases[&room_id].contains(&alias), "should contain the alias for room");

        // Clean up at end
        let _ = storage.remove_room_alias(&room_id).await;
        let _ = storage.delete_room(&room_id).await;
    }
}

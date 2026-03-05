use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Pool, Postgres};
use std::sync::Arc;

const DEFAULT_JOIN_RULE: &str = "invite";
const DEFAULT_HISTORY_VISIBILITY: &str = "joined";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub canonical_alias: Option<String>,
    pub join_rule: String,
    pub creator: Option<String>,
    pub version: String,
    pub encryption: Option<String>,
    pub is_public: bool,
    pub member_count: i64,
    pub history_visibility: String,
    pub creation_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct RoomRecord {
    room_id: String,
    name: Option<String>,
    topic: Option<String>,
    avatar_url: Option<String>,
    canonical_alias: Option<String>,
    join_rules: Option<String>,
    creator: Option<String>,
    room_version: Option<String>,
    encryption: Option<String>,
    is_public: Option<bool>,
    member_count: Option<i64>,
    history_visibility: Option<String>,
    creation_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct RoomWithMembersRecord {
    room_id: String,
    name: Option<String>,
    topic: Option<String>,
    avatar_url: Option<String>,
    canonical_alias: Option<String>,
    join_rules: Option<String>,
    creator: Option<String>,
    room_version: Option<String>,
    encryption: Option<String>,
    is_public: Option<bool>,
    member_count: Option<i64>,
    history_visibility: Option<String>,
    creation_ts: i64,
    joined_members: Option<i64>,
}

#[derive(Clone)]
pub struct RoomStorage {
    pub pool: Arc<Pool<Postgres>>,
}

impl RoomStorage {
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
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<Room, sqlx::Error> {
        let now = chrono::Utc::now().timestamp();

        let query = r#"
            INSERT INTO rooms (room_id, creator, join_rules, room_version, is_public, member_count, history_visibility, creation_ts, last_activity_ts)
            VALUES ($1, $2, $3, $4, $5, 1, 'joined', $6, $7)
            "#;

        if let Some(tx) = tx {
            sqlx::query(query)
                .bind(room_id)
                .bind(creator)
                .bind(join_rule)
                .bind(version)
                .bind(is_public)
                .bind(now)
                .bind(now)
                .execute(&mut **tx)
                .await?;
        } else {
            sqlx::query(query)
                .bind(room_id)
                .bind(creator)
                .bind(join_rule)
                .bind(version)
                .bind(is_public)
                .bind(now)
                .bind(now)
                .execute(&*self.pool)
                .await?;
        }

        // Fetch back the room record. Note: if inside a transaction, we must use the transaction to read it back
        // to see the uncommitted changes, unless read isolation allows otherwise.
        // But sqlx transaction reuse is tricky for select if we want to return the object.
        // For simplicity, we construct the Room object manually since we know what we inserted.

        Ok(Room {
            room_id: room_id.to_string(),
            name: None,
            topic: None,
            avatar_url: None,
            canonical_alias: None,
            join_rule: join_rule.to_string(),
            creator: Some(creator.to_string()),
            version: version.to_string(),
            encryption: None,
            is_public,
            member_count: 1,
            history_visibility: DEFAULT_HISTORY_VISIBILITY.to_string(),
            creation_ts: now,
        })
    }

    pub async fn get_room(&self, room_id: &str) -> Result<Option<Room>, sqlx::Error> {
        let row = sqlx::query_as::<_, RoomRecord>(
            r#"
            SELECT room_id, name, topic, avatar_url, canonical_alias, join_rules, creator, room_version,
                  encryption, is_public, member_count, history_visibility, creation_ts
            FROM rooms WHERE room_id = $1
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
                    .join_rules
                    .unwrap_or_else(|| DEFAULT_JOIN_RULE.to_string()),
                creator: row.creator,
                version: row.room_version.unwrap_or_else(|| "1".to_string()),
                encryption: row.encryption,
                is_public: row.is_public.unwrap_or(false),
                member_count: row.member_count.unwrap_or(0),
                history_visibility: row
                    .history_visibility
                    .unwrap_or_else(|| DEFAULT_HISTORY_VISIBILITY.to_string()),
                creation_ts: row.creation_ts,
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
            SELECT room_id, name, topic, avatar_url, canonical_alias, join_rules, creator, room_version,
                  encryption, is_public, member_count, history_visibility, creation_ts
            FROM rooms
            WHERE room_id = ANY($1)
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
                    .join_rules
                    .clone()
                    .unwrap_or_else(|| DEFAULT_JOIN_RULE.to_string()),
                creator: row.creator.clone(),
                version: row.room_version.clone().unwrap_or_else(|| "1".to_string()),
                encryption: row.encryption.clone(),
                is_public: row.is_public.unwrap_or(false),
                member_count: row.member_count.unwrap_or(0),
                history_visibility: row
                    .history_visibility
                    .clone()
                    .unwrap_or_else(|| DEFAULT_HISTORY_VISIBILITY.to_string()),
                creation_ts: row.creation_ts,
            })
            .collect())
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
        let rows: Vec<RoomRecord> = sqlx::query_as(
            r#"
            SELECT room_id, name, topic, avatar_url, canonical_alias, join_rules, creator, room_version,
                  encryption, is_public, member_count, history_visibility, creation_ts
            FROM rooms WHERE is_public = TRUE
            ORDER BY creation_ts DESC
            LIMIT $1
            "#,
        )
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
                join_rule: row
                    .join_rules
                    .clone()
                    .unwrap_or_else(|| DEFAULT_JOIN_RULE.to_string()),
                creator: row.creator.clone(),
                version: row.room_version.clone().unwrap_or_else(|| "1".to_string()),
                encryption: row.encryption.clone(),
                is_public: row.is_public.unwrap_or(false),
                member_count: row.member_count.unwrap_or(0),
                history_visibility: row
                    .history_visibility
                    .clone()
                    .unwrap_or_else(|| DEFAULT_HISTORY_VISIBILITY.to_string()),
                creation_ts: row.creation_ts,
            })
            .collect())
    }

    pub async fn get_all_rooms_with_members(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<(Room, i64)>, sqlx::Error> {
        let rows: Vec<RoomWithMembersRecord> = sqlx::query_as(
            r#"
            SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias, r.join_rules, r.creator,
                   r.room_version, r.encryption, r.is_public, r.member_count, r.history_visibility,
                   r.creation_ts, COUNT(rm.user_id) as joined_members
            FROM rooms r
            LEFT JOIN room_memberships rm ON r.room_id = rm.room_id AND rm.membership = 'join'
            GROUP BY r.room_id
            ORDER BY r.creation_ts DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|row| {
                (
                    Room {
                        room_id: row.room_id.clone(),
                        name: row.name.clone(),
                        topic: row.topic.clone(),
                        avatar_url: row.avatar_url.clone(),
                        canonical_alias: row.canonical_alias.clone(),
                        join_rule: row
                            .join_rules
                            .clone()
                            .unwrap_or_else(|| DEFAULT_JOIN_RULE.to_string()),
                        creator: row.creator.clone(),
                        version: row.room_version.clone().unwrap_or_else(|| "1".to_string()),
                        encryption: row.encryption.clone(),
                        is_public: row.is_public.unwrap_or(false),
                        member_count: row.member_count.unwrap_or(0),
                        history_visibility: row
                            .history_visibility
                            .clone()
                            .unwrap_or_else(|| DEFAULT_HISTORY_VISIBILITY.to_string()),
                        creation_ts: row.creation_ts,
                    },
                    row.joined_members.unwrap_or(0),
                )
            })
            .collect())
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
        sqlx::query(
            r#"
            UPDATE rooms SET name = $1 WHERE room_id = $2
            "#,
        )
        .bind(name)
        .bind(room_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_room_topic(&self, room_id: &str, topic: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE rooms SET topic = $1 WHERE room_id = $2
            "#,
        )
        .bind(topic)
        .bind(room_id)
        .execute(&*self.pool)
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

    pub async fn update_canonical_alias(
        &self,
        room_id: &str,
        alias: &str,
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

    pub async fn increment_member_count(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE rooms SET member_count = member_count + 1 WHERE room_id = $1
            "#,
        )
        .bind(room_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn decrement_member_count(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE rooms SET member_count = member_count - 1 WHERE room_id = $1 AND member_count > 0
            "#,
        )
        .bind(room_id)
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
        let server_name = "localhost";
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
        sqlx::query(
            r#"
            INSERT INTO room_directory (room_id, is_public)
            VALUES ($1, $2)
            ON CONFLICT (room_id) DO UPDATE SET is_public = EXCLUDED.is_public
            "#,
        )
        .bind(room_id)
        .bind(is_public)
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
        event_type: &str,
        content: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO room_account_data (room_id, event_type, content, created_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (room_id, event_type) DO UPDATE SET content = EXCLUDED.content, created_ts = EXCLUDED.created_ts
            "#,
        )
        .bind(room_id)
        .bind(event_type)
        .bind(content)
        .bind(chrono::Utc::now().timestamp())
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

    pub async fn add_receipt(
        &self,
        _sender: &str,
        sent_to: &str,
        room_id: &str,
        event_id: &str,
        receipt_type: &str,
    ) -> Result<(), sqlx::Error> {
        let now: i64 = chrono::Utc::now().timestamp_millis();
        let receipt_data = json!({});
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
        .bind(sent_to)
        .bind(receipt_type)
        .bind(now)
        .bind(receipt_data)
        .execute(&*self.pool)
        .await?;
        Ok(())
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
                   r.room_version, r.encryption, r.is_public, r.member_count, r.history_visibility,
                   r.creation_ts, COUNT(rm.user_id) as joined_members
            FROM rooms r
            LEFT JOIN room_memberships rm ON r.room_id = rm.room_id AND rm.membership = 'join'
            WHERE r.room_id = ANY($1)
            GROUP BY r.room_id
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
                        .join_rules
                        .clone()
                        .unwrap_or_else(|| DEFAULT_JOIN_RULE.to_string()),
                    creator: row.creator.clone(),
                    version: row.room_version.clone().unwrap_or_else(|| "1".to_string()),
                    encryption: row.encryption.clone(),
                    is_public: row.is_public.unwrap_or(false),
                    member_count: row.member_count.unwrap_or(0),
                    history_visibility: row
                        .history_visibility
                        .clone()
                        .unwrap_or_else(|| DEFAULT_HISTORY_VISIBILITY.to_string()),
                    creation_ts: row.creation_ts,
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

    pub async fn get_public_rooms_with_aliases(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<(Room, Vec<String>)>, sqlx::Error> {
        let rows: Vec<RoomRecord> = sqlx::query_as(
            r#"
            SELECT room_id, name, topic, avatar_url, canonical_alias, join_rules, creator, room_version,
                  encryption, is_public, member_count, history_visibility, creation_ts
            FROM rooms WHERE is_public = TRUE
            ORDER BY creation_ts DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await?;

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
                        .join_rules
                        .clone()
                        .unwrap_or_else(|| DEFAULT_JOIN_RULE.to_string()),
                    creator: row.creator.clone(),
                    version: row.room_version.clone().unwrap_or_else(|| "1".to_string()),
                    encryption: row.encryption.clone(),
                    is_public: row.is_public.unwrap_or(false),
                    member_count: row.member_count.unwrap_or(0),
                    history_visibility: row
                        .history_visibility
                        .clone()
                        .unwrap_or_else(|| DEFAULT_HISTORY_VISIBILITY.to_string()),
                    creation_ts: row.creation_ts,
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
            UPDATE rooms SET member_count = member_count + 1 WHERE room_id = ANY($1)
            "#,
        )
        .bind(room_ids)
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
            UPDATE rooms SET member_count = member_count - 1 WHERE room_id = ANY($1) AND member_count > 0
            "#,
        )
        .bind(room_ids)
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
            creator: Some("@alice:example.com".to_string()),
            version: "6".to_string(),
            encryption: Some("m.megolm.v1.aes-sha2".to_string()),
            is_public: false,
            member_count: 5,
            history_visibility: "joined".to_string(),
            creation_ts: 1234567890,
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
            creator: Some("@bob:example.com".to_string()),
            version: "1".to_string(),
            encryption: None,
            is_public: true,
            member_count: 1,
            history_visibility: DEFAULT_HISTORY_VISIBILITY.to_string(),
            creation_ts: 0,
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
            creator: Some("@test:example.com".to_string()),
            version: "9".to_string(),
            encryption: None,
            is_public: true,
            member_count: 10,
            history_visibility: "shared".to_string(),
            creation_ts: 1234567890,
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
            creator: Some("@admin:example.com".to_string()),
            version: "6".to_string(),
            encryption: Some("m.megolm.v1.aes-sha2".to_string()),
            is_public: false,
            member_count: 3,
            history_visibility: "invited".to_string(),
            creation_ts: 1234567890,
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

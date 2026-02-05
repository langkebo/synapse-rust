use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;

const DEFAULT_JOIN_RULE: &str = "invite";
const DEFAULT_HISTORY_VISIBILITY: &str = "joined";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub canonical_alias: Option<String>,
    pub join_rule: String,
    pub creator: String,
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
    canonical_alias: Option<String>,
    join_rule: Option<String>,
    creator: String,
    version: Option<String>,
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
    canonical_alias: Option<String>,
    join_rule: Option<String>,
    creator: String,
    version: Option<String>,
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
    ) -> Result<Room, sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            r#"
            INSERT INTO rooms (room_id, creator, join_rule, version, is_public, member_count, history_visibility, creation_ts, last_activity_ts)
            VALUES ($1, $2, $3, $4, $5, 1, 'joined', $6, $7)
            "#,
        )
        .bind(room_id)
        .bind(creator)
        .bind(join_rule)
        .bind(version)
        .bind(is_public)
        .bind(now)
        .bind(now)
        .execute(&*self.pool)
        .await?;
        let row = sqlx::query_as::<_, RoomRecord>(
            r#"
            SELECT room_id, name, topic, canonical_alias, join_rule, creator, version,
                   encryption, is_public, member_count, history_visibility, creation_ts
            FROM rooms WHERE room_id = $1
            "#,
        )
        .bind(room_id)
        .fetch_one(&*self.pool)
        .await?;
        Ok(Room {
            room_id: row.room_id,
            name: row.name,
            topic: row.topic,
            canonical_alias: row.canonical_alias,
            join_rule: row
                .join_rule
                .unwrap_or_else(|| DEFAULT_JOIN_RULE.to_string()),
            creator: row.creator,
            version: row.version.unwrap_or_else(|| "1".to_string()),
            encryption: row.encryption,
            is_public: row.is_public.unwrap_or(false),
            member_count: row.member_count.unwrap_or(0) as i64,
            history_visibility: row
                .history_visibility
                .unwrap_or_else(|| DEFAULT_HISTORY_VISIBILITY.to_string()),
            creation_ts: row.creation_ts,
        })
    }

    pub async fn get_room(&self, room_id: &str) -> Result<Option<Room>, sqlx::Error> {
        let row = sqlx::query_as::<_, RoomRecord>(
            r#"
            SELECT room_id, name, topic, canonical_alias, join_rule, creator, version,
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
                canonical_alias: row.canonical_alias,
                join_rule: row
                    .join_rule
                    .unwrap_or_else(|| DEFAULT_JOIN_RULE.to_string()),
                creator: row.creator,
                version: row.version.unwrap_or_else(|| "1".to_string()),
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
            SELECT room_id, name, topic, canonical_alias, join_rule, creator, version,
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
                canonical_alias: row.canonical_alias.clone(),
                join_rule: row
                    .join_rule
                    .clone()
                    .unwrap_or_else(|| DEFAULT_JOIN_RULE.to_string()),
                creator: row.creator.clone(),
                version: row.version.clone().unwrap_or_else(|| "1".to_string()),
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
            SELECT r.room_id, r.name, r.topic, r.canonical_alias, r.join_rule, r.creator,
                   r.version, r.encryption, r.is_public, r.member_count, r.history_visibility,
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
                        canonical_alias: row.canonical_alias.clone(),
                        join_rule: row
                            .join_rule
                            .clone()
                            .unwrap_or_else(|| DEFAULT_JOIN_RULE.to_string()),
                        creator: row.creator.clone(),
                        version: row.version.clone().unwrap_or_else(|| "1".to_string()),
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

    pub async fn set_room_alias(&self, room_id: &str, alias: &str) -> Result<(), sqlx::Error> {
        let created_by = format!("{}:{}", room_id, chrono::Utc::now().timestamp());
        let creation_ts = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"
            INSERT INTO room_aliases (room_alias, alias, room_id, created_by, creation_ts)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (room_alias) DO UPDATE SET
                alias = EXCLUDED.alias,
                room_id = EXCLUDED.room_id,
                created_by = EXCLUDED.created_by,
                creation_ts = EXCLUDED.creation_ts
            "#,
        )
        .bind(alias)
        .bind(alias)
        .bind(room_id)
        .bind(created_by)
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

    pub async fn get_room_alias(&self, room_id: &str) -> Result<Option<String>, sqlx::Error> {
        let result: Option<(String,)> = sqlx::query_as(
            r#"
            SELECT alias FROM room_aliases WHERE room_id = $1
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
            SELECT alias FROM room_aliases WHERE room_id = $1
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
            SELECT room_id FROM room_aliases WHERE alias = $1
            "#,
        )
        .bind(alias)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.map(|r| r.0))
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
        let now: i64 = chrono::Utc::now().timestamp();
        sqlx::query(
            r#"
            INSERT INTO read_markers (room_id, user_id, event_id, created_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (room_id, user_id) DO UPDATE SET event_id = EXCLUDED.event_id, created_ts = EXCLUDED.created_ts
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
        sender: &str,
        sent_to: &str,
        room_id: &str,
        event_id: &str,
        receipt_type: &str,
    ) -> Result<(), sqlx::Error> {
        let now: i64 = chrono::Utc::now().timestamp();
        sqlx::query(
            r#"
            INSERT INTO receipts (sender, sent_to, room_id, event_id, sent_ts, receipt_type)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (sent_to, sender, room_id) DO UPDATE
            SET event_id = EXCLUDED.event_id, sent_ts = EXCLUDED.sent_ts, receipt_type = EXCLUDED.receipt_type
            "#,
        )
        .bind(sender)
        .bind(sent_to)
        .bind(room_id)
        .bind(event_id)
        .bind(now)
        .bind(receipt_type)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }
}

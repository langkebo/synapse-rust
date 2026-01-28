use sqlx::{Pool, Postgres};
use crate::common::*;

#[derive(Debug, Clone)]
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
    pub creation_ts: chrono::DateTime<chrono::Utc>,
}

pub struct RoomStorage<'a> {
    pool: &'a Pool<Postgres>,
}

impl<'a> RoomStorage<'a> {
    pub fn new(pool: &'a Pool<Postgres>) -> Self {
        Self { pool }
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
        sqlx::query_as!(
            Room,
            r#"
            INSERT INTO rooms (room_id, creator, join_rule, version, is_public, member_count, history_visibility, creation_ts)
            VALUES ($1, $2, $3, $4, $5, 1, 'joined', $6)
            RETURNING *
            "#,
            room_id,
            creator,
            join_rule,
            version,
            is_public,
            now
        ).fetch_one(self.pool).await
    }

    pub async fn get_room(&self, room_id: &str) -> Result<Option<Room>, sqlx::Error> {
        sqlx::query_as!(
            Room,
            r#"
            SELECT * FROM rooms WHERE room_id = $1
            "#,
            room_id
        ).fetch_optional(self.pool).await
    }

    pub async fn room_exists(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT 1 FROM rooms WHERE room_id = $1 LIMIT 1
            "#,
            room_id
        ).fetch_optional(self.pool).await?;
        Ok(result.is_some())
    }

    pub async fn get_public_rooms(&self, limit: i64) -> Result<Vec<Room>, sqlx::Error> {
        sqlx::query_as!(
            Room,
            r#"
            SELECT * FROM rooms WHERE is_public = TRUE
            ORDER BY creation_ts DESC
            LIMIT $1
            "#,
            limit
        ).fetch_all(self.pool).await
    }

    pub async fn get_user_rooms(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT room_id FROM room_memberships WHERE user_id = $1 AND membership = 'join'
            "#,
            user_id
        ).fetch_all(self.pool).await?;
        Ok(rows.iter().map(|r| r.room_id.clone()).collect())
    }

    pub async fn update_room_name(&self, room_id: &str, name: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE rooms SET name = $1 WHERE room_id = $2
            "#,
            name,
            room_id
        ).execute(self.pool).await?;
        Ok(())
    }

    pub async fn update_room_topic(&self, room_id: &str, topic: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE rooms SET topic = $1 WHERE room_id = $2
            "#,
            topic,
            room_id
        ).execute(self.pool).await?;
        Ok(())
    }

    pub async fn update_room_avatar(&self, room_id: &str, avatar_url: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE rooms SET name = $1 WHERE room_id = $2
            "#,
            avatar_url,
            room_id
        ).execute(self.pool).await?;
        Ok(())
    }

    pub async fn update_canonical_alias(&self, room_id: &str, alias: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE rooms SET canonical_alias = $1 WHERE room_id = $2
            "#,
            alias,
            room_id
        ).execute(self.pool).await?;
        Ok(())
    }

    pub async fn increment_member_count(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE rooms SET member_count = member_count + 1 WHERE room_id = $1
            "#,
            room_id
        ).execute(self.pool).await?;
        Ok(())
    }

    pub async fn decrement_member_count(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE rooms SET member_count = member_count - 1 WHERE room_id = $1 AND member_count > 0
            "#,
            room_id
        ).execute(self.pool).await?;
        Ok(())
    }

    pub async fn get_room_count(&self) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT COUNT(*) as count FROM rooms
            "#
        ).fetch_one(self.pool).await?;
        Ok(result.count.unwrap_or(0))
    }
}

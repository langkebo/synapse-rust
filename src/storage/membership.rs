use sqlx::{Pool, Postgres};
use crate::common::*;

#[derive(Debug, Clone)]
pub struct RoomMember {
    pub room_id: String,
    pub user_id: String,
    pub display_name: Option<String>,
    pub membership: String,
    pub join_reason: Option<String>,
    pub banned_by: Option<String>,
    pub ban_reason: Option<String>,
    pub ban_ts: Option<chrono::DateTime<chrono::Utc>>,
}

pub struct RoomMemberStorage<'a> {
    pool: &'a Pool<Postgres>,
}

impl<'a> RoomMemberStorage<'a> {
    pub fn new(pool: &'a Pool<Postgres>) -> Self {
        Self { pool }
    }

    pub async fn add_member(
        &self,
        room_id: &str,
        user_id: &str,
        membership: &str,
        display_name: Option<&str>,
        join_reason: Option<&str>,
    ) -> Result<RoomMember, sqlx::Error> {
        sqlx::query_as!(
            RoomMember,
            r#"
            INSERT INTO room_memberships (room_id, user_id, display_name, membership, join_reason)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (room_id, user_id) DO UPDATE SET
                display_name = EXCLUDED.display_name,
                membership = EXCLUDED.membership,
                join_reason = EXCLUDED.join_reason,
                banned_by = NULL,
                ban_reason = NULL,
                ban_ts = NULL
            RETURNING *
            "#,
            room_id,
            user_id,
            display_name,
            membership,
            join_reason
        ).fetch_one(self.pool).await
    }

    pub async fn get_member(&self, room_id: &str, user_id: &str) -> Result<Option<RoomMember>, sqlx::Error> {
        sqlx::query_as!(
            RoomMember,
            r#"
            SELECT * FROM room_memberships WHERE room_id = $1 AND user_id = $2
            "#,
            room_id,
            user_id
        ).fetch_optional(self.pool).await
    }

    pub async fn get_room_members(&self, room_id: &str, membership_type: &str) -> Result<Vec<RoomMember>, sqlx::Error> {
        sqlx::query_as!(
            RoomMember,
            r#"
            SELECT * FROM room_memberships WHERE room_id = $1 AND membership = $2
            "#,
            room_id,
            membership_type
        ).fetch_all(self.pool).await
    }

    pub async fn get_room_member_count(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT COUNT(*) as count FROM room_memberships WHERE room_id = $1 AND membership = 'join'
            "#,
            room_id
        ).fetch_one(self.pool).await?;
        Ok(result.count.unwrap_or(0))
    }

    pub async fn remove_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM room_memberships WHERE room_id = $1 AND user_id = $2
            "#,
            room_id,
            user_id
        ).execute(self.pool).await?;
        Ok(())
    }

    pub async fn ban_member(&self, room_id: &str, user_id: &str, banned_by: &str, reason: Option<&str>) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query!(
            r#"
            INSERT INTO room_memberships (room_id, user_id, membership, banned_by, ban_reason, ban_ts)
            VALUES ($1, $2, 'ban', $3, $4, $5)
            ON CONFLICT (room_id, user_id) DO UPDATE SET
                membership = 'ban',
                banned_by = EXCLUDED.banned_by,
                ban_reason = EXCLUDED.ban_reason,
                ban_ts = EXCLUDED.ban_ts
            "#,
            room_id,
            user_id,
            banned_by,
            reason,
            now
        ).execute(self.pool).await?;
        Ok(())
    }

    pub async fn unban_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE room_memberships SET membership = 'leave', banned_by = NULL, ban_reason = NULL, ban_ts = NULL
            WHERE room_id = $1 AND user_id = $2
            "#,
            room_id,
            user_id
        ).execute(self.pool).await?;
        Ok(())
    }

    pub async fn get_joined_rooms(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT room_id FROM room_memberships WHERE user_id = $1 AND membership = 'join'
            "#,
            user_id
        ).fetch_all(self.pool).await?;
        Ok(rows.iter().map(|r| r.room_id.clone()).collect())
    }

    pub async fn is_member(&self, room_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT 1 FROM room_memberships WHERE room_id = $1 AND user_id = $2 AND membership = 'join' LIMIT 1
            "#,
            room_id,
            user_id
        ).fetch_optional(self.pool).await?;
        Ok(result.is_some())
    }
}

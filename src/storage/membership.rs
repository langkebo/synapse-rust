use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct RoomMember {
    pub room_id: String,
    pub user_id: String,
    pub display_name: Option<String>,
    pub membership: String,
    pub avatar_url: Option<String>,
    pub join_reason: Option<String>,
    pub banned_by: Option<String>,
    pub sender: Option<String>,
    pub event_id: Option<String>,
    pub event_type: Option<String>,
    pub is_banned: Option<bool>,
    pub invite_token: Option<String>,
    pub inviter: Option<String>,
    pub updated_ts: Option<i64>,
    pub joined_ts: Option<i64>,
    pub left_ts: Option<i64>,
    pub reason: Option<String>,
}

#[derive(Clone)]
pub struct RoomMemberStorage {
    pub pool: Arc<Pool<Postgres>>,
}

impl RoomMemberStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
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
                join_reason = EXCLUDED.join_reason
            RETURNING *
            "#,
            room_id,
            user_id,
            display_name,
            membership,
            join_reason
        )
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_member(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<Option<RoomMember>, sqlx::Error> {
        sqlx::query_as!(
            RoomMember,
            r#"
            SELECT * FROM room_memberships WHERE room_id = $1 AND user_id = $2
            "#,
            room_id,
            user_id
        )
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_room_members(
        &self,
        room_id: &str,
        membership_type: &str,
    ) -> Result<Vec<RoomMember>, sqlx::Error> {
        sqlx::query_as!(
            RoomMember,
            r#"
            SELECT * FROM room_memberships WHERE room_id = $1 AND membership = $2
            "#,
            room_id,
            membership_type
        )
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_room_member_count(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT COUNT(*) as count FROM room_memberships WHERE room_id = $1 AND membership = 'join'
            "#,
            room_id
        ).fetch_one(&*self.pool).await?;
        Ok(result.count.unwrap_or(0))
    }

    pub async fn remove_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM room_memberships WHERE room_id = $1 AND user_id = $2
            "#,
            room_id,
            user_id
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn ban_member(
        &self,
        room_id: &str,
        user_id: &str,
        banned_by: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO room_memberships (room_id, user_id, membership, banned_by)
            VALUES ($1, $2, 'ban', $3)
            ON CONFLICT (room_id, user_id) DO UPDATE SET
                membership = 'ban',
                banned_by = EXCLUDED.banned_by
            "#,
            room_id,
            user_id,
            banned_by
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn unban_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE room_memberships SET membership = 'leave', banned_by = NULL
            WHERE room_id = $1 AND user_id = $2
            "#,
            room_id,
            user_id
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_joined_rooms(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT room_id FROM room_memberships WHERE user_id = $1 AND membership = 'join'
            "#,
            user_id
        )
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows.iter().map(|r| r.room_id.clone()).collect())
    }

    pub async fn is_member(&self, room_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT 1 AS "exists" FROM room_memberships WHERE room_id = $1 AND user_id = $2 AND membership = 'join' LIMIT 1
            "#,
            room_id,
            user_id
        ).fetch_optional(&*self.pool).await?;
        Ok(result.is_some())
    }

    pub async fn get_room_member(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<Option<RoomMember>, sqlx::Error> {
        let result = sqlx::query_as!(
            RoomMember,
            r#"
            SELECT * FROM room_memberships WHERE room_id = $1 AND user_id = $2
            "#,
            room_id,
            user_id
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result)
    }

    pub async fn get_joined_members(&self, room_id: &str) -> Result<Vec<RoomMember>, sqlx::Error> {
        let members = sqlx::query_as!(
            RoomMember,
            r#"
            SELECT * FROM room_memberships WHERE room_id = $1 AND membership = 'join'
            "#,
            room_id
        )
        .fetch_all(&*self.pool)
        .await?;
        Ok(members)
    }

    pub async fn get_joined_member(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<Option<RoomMember>, sqlx::Error> {
        let result = sqlx::query_as!(
            RoomMember,
            r#"
            SELECT * FROM room_memberships WHERE room_id = $1 AND user_id = $2 AND membership = 'join'
            "#,
            room_id,
            user_id
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result)
    }
}

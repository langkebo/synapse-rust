use crate::common::crypto::generate_event_id;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct RoomMember {
    pub room_id: String,
    pub user_id: String,
    pub sender: Option<String>,
    pub membership: String,
    pub event_id: Option<String>,
    pub event_type: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub is_banned: Option<bool>,
    pub invite_token: Option<String>,
    pub updated_ts: Option<i64>,
    pub joined_ts: Option<i64>,
    pub left_ts: Option<i64>,
    pub reason: Option<String>,
    pub banned_by: Option<String>,
    pub ban_reason: Option<String>,
    pub ban_ts: Option<i64>,
    pub join_reason: Option<String>,
}

#[derive(Clone)]
pub struct RoomMemberStorage {
    pub pool: Arc<Pool<Postgres>>,
    /// 服务器名称，用于生成事件 ID
    pub server_name: String,
}

impl RoomMemberStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>, server_name: &str) -> Self {
        Self {
            pool: pool.clone(),
            server_name: server_name.to_string(),
        }
    }

    pub async fn add_member(
        &self,
        room_id: &str,
        user_id: &str,
        membership: &str,
        display_name: Option<&str>,
        join_reason: Option<&str>,
    ) -> Result<RoomMember, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let event_id = format!("${}", generate_event_id(&self.server_name));
        let sender = user_id;

        sqlx::query_as::<_, RoomMember>(
            r#"
            INSERT INTO room_memberships (room_id, user_id, sender, membership, event_id, event_type, display_name, join_reason, updated_ts, joined_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (room_id, user_id) DO UPDATE SET
                display_name = EXCLUDED.display_name,
                membership = EXCLUDED.membership,
                join_reason = EXCLUDED.join_reason,
                updated_ts = EXCLUDED.updated_ts
            RETURNING room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, ban_ts, join_reason
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .bind(sender)
        .bind(membership)
        .bind(event_id)
        .bind("m.room.member")
        .bind(display_name)
        .bind(join_reason)
        .bind(now)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_member(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<Option<RoomMember>, sqlx::Error> {
        sqlx::query_as::<_, RoomMember>(
            r#"
            SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, ban_ts, join_reason
            FROM room_memberships WHERE room_id = $1 AND user_id = $2
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_room_members(
        &self,
        room_id: &str,
        membership_type: &str,
    ) -> Result<Vec<RoomMember>, sqlx::Error> {
        sqlx::query_as::<_, RoomMember>(
            r#"
            SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, ban_ts, join_reason
            FROM room_memberships WHERE room_id = $1 AND membership = $2
            "#,
        )
        .bind(room_id)
        .bind(membership_type)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_room_member_count(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COALESCE(COUNT(*), 0) FROM room_memberships WHERE room_id = $1 AND membership = 'join'
            "#,
        )
        .bind(room_id)
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
    }

    pub async fn remove_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM room_memberships WHERE room_id = $1 AND user_id = $2
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove_all_members(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM room_memberships WHERE room_id = $1
            "#,
        )
        .bind(room_id)
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
        sqlx::query(
            r#"
            INSERT INTO room_memberships (room_id, user_id, membership, banned_by)
            VALUES ($1, $2, 'ban', $3)
            ON CONFLICT (room_id, user_id) DO UPDATE SET
                membership = 'ban',
                banned_by = EXCLUDED.banned_by
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .bind(banned_by)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn unban_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE room_memberships SET membership = 'leave', banned_by = NULL
            WHERE room_id = $1 AND user_id = $2
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_joined_rooms(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
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

    pub async fn is_member(&self, room_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar::<_, i32>(
            r#"
            SELECT 1 AS "exists" FROM room_memberships WHERE room_id = $1 AND user_id = $2 AND membership = 'join' LIMIT 1
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.is_some())
    }

    pub async fn get_room_member(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<Option<RoomMember>, sqlx::Error> {
        let result = sqlx::query_as::<_, RoomMember>(
            r#"
            SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, ban_ts, join_reason
            FROM room_memberships WHERE room_id = $1 AND user_id = $2
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result)
    }

    pub async fn get_joined_members(&self, room_id: &str) -> Result<Vec<RoomMember>, sqlx::Error> {
        let members = sqlx::query_as::<_, RoomMember>(
            r#"
            SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, ban_ts, join_reason
            FROM room_memberships WHERE room_id = $1 AND membership = 'join'
            "#,
        )
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;
        Ok(members)
    }

    pub async fn get_joined_member(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<Option<RoomMember>, sqlx::Error> {
        let result = sqlx::query_as::<_, RoomMember>(
            r#"
            SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, ban_ts, join_reason
            FROM room_memberships WHERE room_id = $1 AND user_id = $2 AND membership = 'join'
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result)
    }

    pub async fn share_common_room(
        &self,
        user_id_1: &str,
        user_id_2: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar::<_, i32>(
            r#"
            SELECT 1 FROM room_memberships m1
            JOIN room_memberships m2 ON m1.room_id = m2.room_id
            WHERE m1.user_id = $1 AND m1.membership = 'join'
              AND m2.user_id = $2 AND m2.membership = 'join'
            LIMIT 1
            "#,
        )
        .bind(user_id_1)
        .bind(user_id_2)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.is_some())
    }

    pub async fn get_membership_history(
        &self,
        room_id: &str,
        limit: i64,
    ) -> Result<Vec<RoomMember>, sqlx::Error> {
        let memberships = sqlx::query_as::<_, RoomMember>(
            r#"
            SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, ban_ts, join_reason
            FROM room_memberships WHERE room_id = $1
            ORDER BY updated_ts DESC
            LIMIT $2
            "#,
        )
        .bind(room_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;
        Ok(memberships)
    }
}

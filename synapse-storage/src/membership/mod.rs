pub(crate) mod api;

pub use api::MemberStoreApi;

use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use synapse_common::crypto::generate_event_id;

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
    pub banned_ts: Option<i64>,
    pub join_reason: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserRoomMembership {
    pub room_id: String,
    pub membership: String,
}

#[derive(Clone)]
pub struct RoomMemberStorage {
    pub pool: Arc<Pool<Postgres>>,
    /// 服务器名称，用于生成事件 ID
    pub server_name: String,
}

impl RoomMemberStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>, server_name: &str) -> Self {
        Self { pool: pool.clone(), server_name: server_name.to_string() }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn add_member(
        &self,
        room_id: &str,
        user_id: &str,
        membership: &str,
        display_name: Option<&str>,
        join_reason: Option<&str>,
        sender: Option<&str>,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomMember, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let event_id = format!("${}", generate_event_id(&self.server_name));
        let joined_ts = if membership == "join" { Some(now) } else { None };
        // Use explicit sender if provided (e.g. inviter), otherwise default to user_id
        let effective_sender = sender.unwrap_or(user_id);

        let query = r"
            INSERT INTO room_memberships (room_id, user_id, sender, membership, event_id, event_type, display_name, join_reason, updated_ts, joined_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (room_id, user_id) DO UPDATE SET
                display_name = EXCLUDED.display_name,
                membership = EXCLUDED.membership,
                join_reason = EXCLUDED.join_reason,
                updated_ts = EXCLUDED.updated_ts,
                joined_ts = CASE
                    WHEN EXCLUDED.membership = 'join' THEN COALESCE(room_memberships.joined_ts, EXCLUDED.joined_ts)
                    ELSE room_memberships.joined_ts
                END,
                left_ts = CASE
                    WHEN EXCLUDED.membership = 'join' THEN NULL
                    ELSE room_memberships.left_ts
                END
            RETURNING room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            ";

        if let Some(tx) = tx {
            sqlx::query_as::<_, RoomMember>(query)
                .bind(room_id)
                .bind(user_id)
                .bind(effective_sender)
                .bind(membership)
                .bind(event_id)
                .bind("m.room.member")
                .bind(display_name)
                .bind(join_reason)
                .bind(now)
                .bind(joined_ts)
                .fetch_one(&mut **tx)
                .await
        } else {
            sqlx::query_as::<_, RoomMember>(query)
                .bind(room_id)
                .bind(user_id)
                .bind(effective_sender)
                .bind(membership)
                .bind(event_id)
                .bind("m.room.member")
                .bind(display_name)
                .bind(join_reason)
                .bind(now)
                .bind(joined_ts)
                .fetch_one(&*self.pool)
                .await
        }
    }

    pub async fn get_member(&self, room_id: &str, user_id: &str) -> Result<Option<RoomMember>, sqlx::Error> {
        sqlx::query_as::<_, RoomMember>(
            r"
            SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            FROM room_memberships WHERE room_id = $1 AND user_id = $2
            ",
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_room_members(&self, room_id: &str, membership_type: &str) -> Result<Vec<RoomMember>, sqlx::Error> {
        sqlx::query_as::<_, RoomMember>(
            r"
            SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            FROM room_memberships WHERE room_id = $1 AND membership = $2
            ",
        )
        .bind(room_id)
        .bind(membership_type)
        .fetch_all(&*self.pool)
        .await
    }

    /// Check whether any user from the given server domain has a non-banned
    /// membership (join, invite, or leave) in the room. This is used for
    /// federation authorization—aligns with Synapse v1.153 which allows
    /// servers with any non-banned members to access room state/backfill.
    pub async fn has_any_non_banned_member_from_server(
        &self,
        room_id: &str,
        server_name: &str,
    ) -> Result<bool, sqlx::Error> {
        let domain_pattern = format!("%:{}", server_name);
        let exists: Option<bool> = sqlx::query_scalar(
            r"
            SELECT EXISTS(
                SELECT 1 FROM room_memberships
                WHERE room_id = $1
                  AND user_id LIKE $2
                  AND membership IN ('join', 'invite', 'leave')
            )
            ",
        )
        .bind(room_id)
        .bind(&domain_pattern)
        .fetch_one(&*self.pool)
        .await?;
        Ok(exists.unwrap_or(false))
    }

    pub async fn get_room_member_count(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar::<_, i64>(
            r"
            SELECT COALESCE(COUNT(*), 0) FROM room_memberships WHERE room_id = $1 AND membership = 'join'
            ",
        )
        .bind(room_id)
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
    }

    pub async fn get_room_members_paginated(
        &self,
        room_id: &str,
        membership_type: &str,
        limit: i64,
        from_user_id: Option<&str>,
    ) -> Result<Vec<RoomMember>, sqlx::Error> {
        if let Some(from_user_id) = from_user_id {
            sqlx::query_as::<_, RoomMember>(
                r"
                SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
                FROM room_memberships
                WHERE room_id = $1 AND membership = $2 AND user_id > $3
                ORDER BY user_id ASC
                LIMIT $4
                ",
            )
            .bind(room_id)
            .bind(membership_type)
            .bind(from_user_id)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        } else {
            sqlx::query_as::<_, RoomMember>(
                r"
                SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
                FROM room_memberships
                WHERE room_id = $1 AND membership = $2
                ORDER BY user_id ASC
                LIMIT $3
                ",
            )
            .bind(room_id)
            .bind(membership_type)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        }
    }

    pub async fn remove_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r"
            UPDATE room_memberships
            SET membership = 'leave',
                left_ts = $3,
                updated_ts = $3,
                is_banned = false
            WHERE room_id = $1 AND user_id = $2 AND membership IN ('join', 'ban', 'invite')
            ",
        )
        .bind(room_id)
        .bind(user_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn forget_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r"
            UPDATE room_memberships
            SET membership = 'forget',
                left_ts = $3,
                updated_ts = $3
            WHERE room_id = $1 AND user_id = $2 AND membership IN ('leave', 'invite')
            ",
        )
        .bind(room_id)
        .bind(user_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn is_forgotten(&self, room_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar::<_, i32>(
            r#"
            SELECT 1 AS "exists" FROM room_memberships
            WHERE room_id = $1 AND user_id = $2 AND membership = 'forget'
            LIMIT 1
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.is_some())
    }

    pub async fn get_shared_room_users(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar::<_, String>(
            r"
            SELECT DISTINCT m2.user_id
            FROM room_memberships m1
            JOIN room_memberships m2 ON m1.room_id = m2.room_id
            WHERE m1.user_id = $1 AND m1.membership = 'join'
              AND m2.membership = 'join'
              AND m2.user_id != $1
            ",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn remove_all_members(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            DELETE FROM room_memberships WHERE room_id = $1
            ",
        )
        .bind(room_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn ban_member(&self, room_id: &str, user_id: &str, banned_by: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            INSERT INTO room_memberships (room_id, user_id, membership, banned_by)
            VALUES ($1, $2, 'ban', $3)
            ON CONFLICT (room_id, user_id) DO UPDATE SET
                membership = 'ban',
                banned_by = EXCLUDED.banned_by
            ",
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
            r"
            UPDATE room_memberships SET membership = 'leave', banned_by = NULL
            WHERE room_id = $1 AND user_id = $2 AND membership = 'ban'
            ",
        )
        .bind(room_id)
        .bind(user_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_joined_rooms(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<String> = sqlx::query_scalar::<_, String>(
            r"
            SELECT room_id FROM room_memberships WHERE user_id = $1 AND membership = 'join'
            ",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_sync_rooms(
        &self,
        user_id: &str,
        include_leave: bool,
    ) -> Result<Vec<UserRoomMembership>, sqlx::Error> {
        let memberships = if include_leave {
            sqlx::query_as::<_, UserRoomMembership>(
                r"
                SELECT room_id, membership
                FROM room_memberships
                WHERE user_id = $1 AND membership IN ('join', 'leave')
                ORDER BY updated_ts DESC NULLS LAST, room_id ASC
                ",
            )
            .bind(user_id)
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query_as::<_, UserRoomMembership>(
                r"
                SELECT room_id, membership
                FROM room_memberships
                WHERE user_id = $1 AND membership = 'join'
                ORDER BY updated_ts DESC NULLS LAST, room_id ASC
                ",
            )
            .bind(user_id)
            .fetch_all(&*self.pool)
            .await?
        };

        Ok(memberships)
    }

    pub async fn get_membership_state(&self, room_id: &str, user_id: &str) -> Result<Option<String>, sqlx::Error> {
        let result: Option<(String,)> = sqlx::query_as(
            r"
            SELECT membership FROM room_memberships WHERE room_id = $1 AND user_id = $2
            ",
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.map(|r| r.0))
    }

    pub async fn get_joined_room_count(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar::<_, i64>(
            r"
            SELECT COUNT(*) FROM room_memberships WHERE user_id = $1 AND membership = 'join'
            ",
        )
        .bind(user_id)
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
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

    pub async fn get_room_member(&self, room_id: &str, user_id: &str) -> Result<Option<RoomMember>, sqlx::Error> {
        let result = sqlx::query_as::<_, RoomMember>(
            r"
            SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            FROM room_memberships WHERE room_id = $1 AND user_id = $2
            ",
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result)
    }

    pub async fn get_joined_members(&self, room_id: &str) -> Result<Vec<RoomMember>, sqlx::Error> {
        let members = sqlx::query_as::<_, RoomMember>(
            r"
            SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            FROM room_memberships WHERE room_id = $1 AND membership = 'join'
            ",
        )
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;
        Ok(members)
    }

    pub async fn get_joined_member(&self, room_id: &str, user_id: &str) -> Result<Option<RoomMember>, sqlx::Error> {
        let result = sqlx::query_as::<_, RoomMember>(
            r"
            SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            FROM room_memberships WHERE room_id = $1 AND user_id = $2 AND membership = 'join'
            ",
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result)
    }

    pub async fn share_common_room(&self, user_id_1: &str, user_id_2: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar::<_, i32>(
            r"
            SELECT 1 FROM room_memberships m1
            JOIN room_memberships m2 ON m1.room_id = m2.room_id
            WHERE m1.user_id = $1 AND m1.membership = 'join'
              AND m2.user_id = $2 AND m2.membership = 'join'
            LIMIT 1
            ",
        )
        .bind(user_id_1)
        .bind(user_id_2)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.is_some())
    }

    pub async fn share_common_rooms_batch(
        &self,
        user_id: &str,
        other_user_ids: &[String],
    ) -> Result<Vec<String>, sqlx::Error> {
        if other_user_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows: Vec<(String,)> = sqlx::query_as(
            r"
            SELECT DISTINCT m2.user_id
            FROM room_memberships m1
            JOIN room_memberships m2 ON m1.room_id = m2.room_id
            WHERE m1.user_id = $1 AND m1.membership = 'join'
              AND m2.user_id = ANY($2) AND m2.membership = 'join'
            ",
        )
        .bind(user_id)
        .bind(other_user_ids)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|(uid,)| uid).collect())
    }

    pub async fn get_membership_history(&self, room_id: &str, limit: i64) -> Result<Vec<RoomMember>, sqlx::Error> {
        let memberships = sqlx::query_as::<_, RoomMember>(
            r"
            SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            FROM room_memberships WHERE room_id = $1
            ORDER BY updated_ts DESC
            LIMIT $2
            ",
        )
        .bind(room_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;
        Ok(memberships)
    }

    pub async fn get_joined_rooms_with_details(
        &self,
        user_id: &str,
    ) -> Result<Vec<(String, String, Option<String>, Option<String>)>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
            r"
            SELECT r.room_id, r.name, r.topic, r.avatar_url
            FROM room_memberships rm
            JOIN rooms r ON rm.room_id = r.room_id
            WHERE rm.user_id = $1 AND rm.membership = 'join'
            ORDER BY r.created_ts DESC
            ",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn get_room_members_with_profiles(
        &self,
        room_id: &str,
        membership_type: &str,
    ) -> Result<Vec<(RoomMember, Option<String>, Option<String>)>, sqlx::Error> {
        let rows = sqlx::query(
            r"
            SELECT rm.room_id, rm.user_id, rm.sender, rm.membership, rm.event_id, rm.event_type,
                   rm.display_name, rm.avatar_url, rm.is_banned, rm.invite_token, rm.updated_ts,
                   rm.joined_ts, rm.left_ts, rm.reason, rm.banned_by, rm.ban_reason, rm.banned_ts, rm.join_reason,
                   u.displayname as user_displayname, u.avatar_url as user_avatar_url
            FROM room_memberships rm
            LEFT JOIN users u ON rm.user_id = u.user_id
            WHERE rm.room_id = $1 AND rm.membership = $2
            ",
        )
        .bind(room_id)
        .bind(membership_type)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|row| {
                use sqlx::Row;
                let member = RoomMember {
                    room_id: row.get("room_id"),
                    user_id: row.get("user_id"),
                    sender: row.get("sender"),
                    membership: row.get("membership"),
                    event_id: row.get("event_id"),
                    event_type: row.get("event_type"),
                    display_name: row.get("display_name"),
                    avatar_url: row.get("avatar_url"),
                    is_banned: row.get("is_banned"),
                    invite_token: row.get("invite_token"),
                    updated_ts: row.get("updated_ts"),
                    joined_ts: row.get("joined_ts"),
                    left_ts: row.get("left_ts"),
                    reason: row.get("reason"),
                    banned_by: row.get("banned_by"),
                    ban_reason: row.get("ban_reason"),
                    banned_ts: row.get("banned_ts"),
                    join_reason: row.get("join_reason"),
                };
                let user_displayname: Option<String> = row.get("user_displayname");
                let user_avatar_url: Option<String> = row.get("user_avatar_url");
                (member, user_displayname, user_avatar_url)
            })
            .collect())
    }

    pub async fn get_members_batch(
        &self,
        room_ids: &[String],
        membership_type: &str,
    ) -> Result<std::collections::HashMap<String, Vec<RoomMember>>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let rows: Vec<RoomMember> = sqlx::query_as(
            r"
            SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            FROM room_memberships
            WHERE room_id = ANY($1) AND membership = $2
            ",
        )
        .bind(room_ids)
        .bind(membership_type)
        .fetch_all(&*self.pool)
        .await?;

        let mut result: std::collections::HashMap<String, Vec<RoomMember>> =
            room_ids.iter().map(|id| (id.clone(), Vec::new())).collect();

        for member in rows {
            if let Some(room_members) = result.get_mut(&member.room_id) {
                room_members.push(member);
            }
        }

        Ok(result)
    }

    pub async fn get_joined_members_batch(
        &self,
        room_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<RoomMember>>, sqlx::Error> {
        self.get_members_batch(room_ids, "join").await
    }

    pub async fn check_membership_batch(
        &self,
        room_id: &str,
        user_ids: &[String],
        membership_type: &str,
    ) -> Result<std::collections::HashSet<String>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(std::collections::HashSet::new());
        }

        let rows: Vec<String> = sqlx::query_scalar(
            r"
            SELECT user_id FROM room_memberships
            WHERE room_id = $1 AND user_id = ANY($2) AND membership = $3
            ",
        )
        .bind(room_id)
        .bind(user_ids)
        .bind(membership_type)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().collect())
    }

    /// Check whether a given user shares any joined room with a member from the
    /// given server domain. Uses a self-join on `room_memberships` with an
    /// EXISTS subquery to avoid the N+1 pattern of fetching all joined rooms
    /// then querying members per room.
    pub async fn user_shares_room_with_server(&self, user_id: &str, server_name: &str) -> Result<bool, sqlx::Error> {
        let domain_pattern = format!("%:{}", server_name);
        let exists: Option<bool> = sqlx::query_scalar(
            r"
            SELECT EXISTS(
                SELECT 1
                FROM room_memberships m1
                JOIN room_memberships m2 ON m1.room_id = m2.room_id
                WHERE m1.user_id = $1
                  AND m1.membership = 'join'
                  AND m2.membership = 'join'
                  AND m2.user_id LIKE $2
            )
            ",
        )
        .bind(user_id)
        .bind(&domain_pattern)
        .fetch_one(&*self.pool)
        .await?;
        Ok(exists.unwrap_or(false))
    }

    /// Batch version of `user_shares_room_with_server`: returns the subset of
    /// `user_ids` that share at least one joined room with a member from the
    /// given server domain. Uses a single query with `ANY($1)` to avoid the
    /// nested N+1 pattern when validating multiple users in federation
    /// `keys_claim` / `keys_query` endpoints.
    pub async fn filter_users_sharing_room_with_server(
        &self,
        user_ids: &[String],
        server_name: &str,
    ) -> Result<std::collections::HashSet<String>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(std::collections::HashSet::new());
        }

        let domain_pattern = format!("%:{}", server_name);
        let rows: Vec<String> = sqlx::query_scalar(
            r"
            SELECT DISTINCT m1.user_id
            FROM room_memberships m1
            JOIN room_memberships m2 ON m1.room_id = m2.room_id
            WHERE m1.user_id = ANY($1)
              AND m1.membership = 'join'
              AND m2.membership = 'join'
              AND m2.user_id LIKE $2
            ",
        )
        .bind(user_ids)
        .bind(&domain_pattern)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().collect())
    }

    pub async fn set_ban_reason(&self, room_id: &str, user_id: &str, reason: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE room_memberships
            SET ban_reason = $3
            WHERE room_id = $1 AND user_id = $2
            ",
        )
        .bind(room_id)
        .bind(user_id)
        .bind(reason)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn force_leave_membership(&self, room_id: &str, user_id: &str, now: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r"
            UPDATE room_memberships
            SET membership = 'leave',
                left_ts = $3,
                updated_ts = $3
            WHERE room_id = $1 AND user_id = $2
            ",
        )
        .bind(room_id)
        .bind(user_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// Returns the distinct server domains of all currently-joined members
    /// in a room.  Used to build the candidate-server list for outbound
    /// `/backfill` — servers with a joined member are guaranteed to have
    /// the room's history (per Matrix federation invariants).
    ///
    /// The local server name is excluded from the result, since we never
    /// need to backfill from ourselves.
    pub async fn get_joined_servers_in_room(
        &self,
        room_id: &str,
        local_server_name: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<String> = sqlx::query_scalar(
            r"
            SELECT DISTINCT
                SUBSTRING(user_id FROM POSITION(':' IN user_id) + 1) AS server_name
            FROM room_memberships
            WHERE room_id = $1
              AND membership = 'join'
              AND user_id LIKE '%:%'
            ",
        )
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows.into_iter().filter(|s| s != local_server_name).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_room_member_struct() {
        let member = RoomMember {
            room_id: "!room123:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            sender: Some("@alice:example.com".to_string()),
            membership: "join".to_string(),
            event_id: Some("$event123:example.com".to_string()),
            event_type: Some("m.room.member".to_string()),
            display_name: Some("Alice".to_string()),
            avatar_url: Some("mxc://example.com/avatar".to_string()),
            is_banned: Some(false),
            invite_token: None,
            updated_ts: Some(1234567890),
            joined_ts: Some(1234567890),
            left_ts: None,
            reason: None,
            banned_by: None,
            ban_reason: None,
            banned_ts: None,
            join_reason: None,
        };

        assert_eq!(member.room_id, "!room123:example.com");
        assert_eq!(member.user_id, "@alice:example.com");
        assert_eq!(member.membership, "join");
        assert!(member.is_banned.is_some());
        assert!(!member.is_banned.unwrap());
    }

    #[test]
    fn test_membership_types() {
        let join_membership = "join";
        let leave_membership = "leave";
        let invite_membership = "invite";
        let ban_membership = "ban";
        let forget_membership = "forget";

        assert_eq!(join_membership, "join");
        assert_eq!(leave_membership, "leave");
        assert_eq!(invite_membership, "invite");
        assert_eq!(ban_membership, "ban");
        assert_eq!(forget_membership, "forget");
    }

    #[test]
    fn test_room_member_banned() {
        let banned_member = RoomMember {
            room_id: "!room:example.com".to_string(),
            user_id: "@bob:example.com".to_string(),
            sender: Some("@admin:example.com".to_string()),
            membership: "ban".to_string(),
            event_id: Some("$ban_event:example.com".to_string()),
            event_type: Some("m.room.member".to_string()),
            display_name: None,
            avatar_url: None,
            is_banned: Some(true),
            invite_token: None,
            updated_ts: Some(1234567890),
            joined_ts: None,
            left_ts: Some(1234567890),
            reason: Some("Spam".to_string()),
            banned_by: Some("@admin:example.com".to_string()),
            ban_reason: Some("Spam behavior".to_string()),
            banned_ts: Some(1234567890),
            join_reason: None,
        };

        assert_eq!(banned_member.membership, "ban");
        assert!(banned_member.is_banned.unwrap_or(false));
        assert!(banned_member.banned_by.is_some());
        assert!(banned_member.ban_reason.is_some());
    }

    #[test]
    fn test_room_member_invited() {
        let invited_member = RoomMember {
            room_id: "!room:example.com".to_string(),
            user_id: "@charlie:example.com".to_string(),
            sender: Some("@alice:example.com".to_string()),
            membership: "invite".to_string(),
            event_id: Some("$invite_event:example.com".to_string()),
            event_type: Some("m.room.member".to_string()),
            display_name: Some("Charlie".to_string()),
            avatar_url: None,
            is_banned: Some(false),
            invite_token: Some("token123".to_string()),
            updated_ts: Some(1234567890),
            joined_ts: None,
            left_ts: None,
            reason: None,
            banned_by: None,
            ban_reason: None,
            banned_ts: None,
            join_reason: None,
        };

        assert_eq!(invited_member.membership, "invite");
        assert!(invited_member.invite_token.is_some());
        assert!(invited_member.joined_ts.is_none());
    }

    #[test]
    fn test_room_member_left() {
        let left_member = RoomMember {
            room_id: "!room:example.com".to_string(),
            user_id: "@dave:example.com".to_string(),
            sender: Some("@dave:example.com".to_string()),
            membership: "leave".to_string(),
            event_id: Some("$leave_event:example.com".to_string()),
            event_type: Some("m.room.member".to_string()),
            display_name: Some("Dave".to_string()),
            avatar_url: None,
            is_banned: Some(false),
            invite_token: None,
            updated_ts: Some(1234567900),
            joined_ts: Some(1234567800),
            left_ts: Some(1234567900),
            reason: Some("Leaving room".to_string()),
            banned_by: None,
            ban_reason: None,
            banned_ts: None,
            join_reason: None,
        };

        assert_eq!(left_member.membership, "leave");
        assert!(left_member.left_ts.is_some());
        assert!(left_member.joined_ts.is_some());
    }

    #[test]
    fn test_room_member_serialization() {
        let member = RoomMember {
            room_id: "!room:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            sender: Some("@user:example.com".to_string()),
            membership: "join".to_string(),
            event_id: Some("$event:example.com".to_string()),
            event_type: Some("m.room.member".to_string()),
            display_name: Some("User".to_string()),
            avatar_url: Some("mxc://example.com/avatar".to_string()),
            is_banned: Some(false),
            invite_token: None,
            updated_ts: Some(1234567890),
            joined_ts: Some(1234567890),
            left_ts: None,
            reason: None,
            banned_by: None,
            ban_reason: None,
            banned_ts: None,
            join_reason: None,
        };

        let json = serde_json::to_string(&member).unwrap();
        assert!(json.contains("join"));
        assert!(json.contains("@user:example.com"));
        assert!(json.contains("!room:example.com"));
    }

    #[test]
    fn test_room_member_storage_creation() {
        let member = RoomMember {
            room_id: "!test:example.com".to_string(),
            user_id: "@test:example.com".to_string(),
            sender: None,
            membership: "join".to_string(),
            event_id: None,
            event_type: None,
            display_name: None,
            avatar_url: None,
            is_banned: None,
            invite_token: None,
            updated_ts: None,
            joined_ts: None,
            left_ts: None,
            reason: None,
            banned_by: None,
            ban_reason: None,
            banned_ts: None,
            join_reason: None,
        };

        assert_eq!(member.membership, "join");
        assert!(member.sender.is_none());
        assert!(member.event_id.is_none());
    }
}

#[cfg(test)]
#[allow(clippy::useless_conversion, clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod db_tests {
    use super::*;
    use std::env;

    async fn test_pool() -> Arc<sqlx::Pool<sqlx::Postgres>> {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&db_url)
            .await
            .expect("Failed to connect to test database");
        Arc::new(pool)
    }

    async fn ensure_test_room(pool: &sqlx::PgPool, room_id: &str) {
        sqlx::query(
            "INSERT INTO rooms (room_id, room_version, is_public, creator, created_ts) \
             VALUES ($1, '1', false, '@test:localhost', (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT) \
             ON CONFLICT (room_id) DO NOTHING",
        )
        .bind(room_id)
        .execute(pool)
        .await
        .expect("failed to create test room");
    }

    async fn ensure_test_user(pool: &sqlx::PgPool, user_id: &str) {
        let now = chrono::Utc::now().timestamp_millis();
        let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
        sqlx::query(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) \
             ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(username)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test user");
    }

    async fn cleanup_membership_data(pool: &sqlx::PgPool, suffix: &str) {
        let pattern = format!("%{suffix}%");
        sqlx::query("DELETE FROM room_memberships WHERE user_id LIKE $1").bind(&pattern).execute(pool).await.ok();
        sqlx::query("DELETE FROM room_memberships WHERE room_id LIKE $1").bind(&pattern).execute(pool).await.ok();
        sqlx::query("DELETE FROM rooms WHERE room_id LIKE $1").bind(&pattern).execute(pool).await.ok();
        sqlx::query("DELETE FROM users WHERE user_id LIKE $1").bind(&pattern).execute(pool).await.ok();
    }

    // ── 1. add_member ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_add_member_all_membership_types() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mem_test_types_{suffix}:localhost");
        let room_id = format!("!room_test_types_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;

        // Join
        let m = storage.add_member(&room_id, &user_id, "join", Some("TN"), None, None, None).await.unwrap();
        assert_eq!(m.membership, "join");
        assert_eq!(m.display_name.as_deref(), Some("TN"));
        assert!(m.joined_ts.is_some());

        // Invite (upsert)
        let m = storage.add_member(&room_id, &user_id, "invite", None, None, None, None).await.unwrap();
        assert_eq!(m.membership, "invite");

        // Ban (upsert)
        let m = storage.add_member(&room_id, &user_id, "ban", None, None, None, None).await.unwrap();
        assert_eq!(m.membership, "ban");

        // Leave (upsert)
        let m = storage.add_member(&room_id, &user_id, "leave", None, None, None, None).await.unwrap();
        assert_eq!(m.membership, "leave");

        cleanup_membership_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_add_member_with_sender() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mem_sender_{suffix}:localhost");
        let sender = format!("@mem_inviter_{suffix}:localhost");
        let room_id = format!("!room_sender_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;
        ensure_test_user(&pool, &sender).await;

        let m = storage.add_member(&room_id, &user_id, "invite", None, None, Some(&sender), None).await.unwrap();
        assert_eq!(m.sender.as_deref(), Some(sender.as_str()));

        cleanup_membership_data(&pool, &suffix).await;
    }

    // ── 2. get_member ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_member_found_and_not_found() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mem_found_{suffix}:localhost");
        let room_id = format!("!room_found_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;

        storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();

        // Found
        let result = storage.get_member(&room_id, &user_id).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().membership, "join");

        // Not found — non-existent user
        let result = storage.get_member(&room_id, "@nonexistent:localhost").await.unwrap();
        assert!(result.is_none());

        // Not found — non-existent room
        let result = storage.get_member("!nonexistent:localhost", &user_id).await.unwrap();
        assert!(result.is_none());

        cleanup_membership_data(&pool, &suffix).await;
    }

    // ── 3. get_room_members ───────────────────────────────────────

    #[tokio::test]
    async fn test_get_room_members_filtered_by_type() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_a = format!("@mem_filter_a_{suffix}:localhost");
        let user_b = format!("@mem_filter_b_{suffix}:localhost");
        let room_id = format!("!room_filter_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_a).await;
        ensure_test_user(&pool, &user_b).await;

        storage.add_member(&room_id, &user_a, "join", None, None, None, None).await.unwrap();
        storage.add_member(&room_id, &user_b, "invite", None, None, None, None).await.unwrap();

        let joins = storage.get_room_members(&room_id, "join").await.unwrap();
        assert_eq!(joins.len(), 1);
        assert_eq!(joins[0].user_id, user_a);

        let invites = storage.get_room_members(&room_id, "invite").await.unwrap();
        assert_eq!(invites.len(), 1);
        assert_eq!(invites[0].user_id, user_b);

        let leaves = storage.get_room_members(&room_id, "leave").await.unwrap();
        assert!(leaves.is_empty());

        cleanup_membership_data(&pool, &suffix).await;
    }

    // ── 4. has_any_non_banned_member_from_server ──────────────────

    #[tokio::test]
    async fn test_has_any_non_banned_member_from_server_true() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mem_srv_true_{suffix}:localhost");
        let room_id = format!("!room_srv_true_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;

        storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();

        let has = storage.has_any_non_banned_member_from_server(&room_id, "localhost").await.unwrap();
        assert!(has);

        cleanup_membership_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_has_any_non_banned_member_from_server_false() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mem_srv_false_{suffix}:localhost");
        let room_id = format!("!room_srv_false_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;

        storage.add_member(&room_id, &user_id, "ban", None, None, None, None).await.unwrap();

        let has = storage.has_any_non_banned_member_from_server(&room_id, "localhost").await.unwrap();
        assert!(!has);

        // Also false for a server with no members at all
        let has_other = storage.has_any_non_banned_member_from_server(&room_id, "otherhost").await.unwrap();
        assert!(!has_other);

        cleanup_membership_data(&pool, &suffix).await;
    }

    // ── 5. get_room_member_count ───────────────────────────────────

    #[tokio::test]
    async fn test_get_room_member_count() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_a = format!("@mem_count_a_{suffix}:localhost");
        let user_b = format!("@mem_count_b_{suffix}:localhost");
        let room_id = format!("!room_count_{suffix}:localhost");
        let empty_room = format!("!room_count_empty_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_room(&pool, &empty_room).await;
        ensure_test_user(&pool, &user_a).await;
        ensure_test_user(&pool, &user_b).await;

        // Empty room
        let count = storage.get_room_member_count(&empty_room).await.unwrap();
        assert_eq!(count, 0);

        // With join members
        storage.add_member(&room_id, &user_a, "join", None, None, None, None).await.unwrap();
        storage.add_member(&room_id, &user_b, "join", None, None, None, None).await.unwrap();
        let count = storage.get_room_member_count(&room_id).await.unwrap();
        assert_eq!(count, 2);

        // Leave member should not count
        storage.add_member(&room_id, &user_b, "leave", None, None, None, None).await.unwrap();
        let count = storage.get_room_member_count(&room_id).await.unwrap();
        assert_eq!(count, 1);

        cleanup_membership_data(&pool, &suffix).await;
    }

    // ── 6. get_room_members_paginated ──────────────────────────────

    #[tokio::test]
    async fn test_get_room_members_paginated() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_0 = format!("@mem_page_0_{suffix}:localhost");
        let user_1 = format!("@mem_page_1_{suffix}:localhost");
        let user_2 = format!("@mem_page_2_{suffix}:localhost");
        let room_id = format!("!room_page_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_0).await;
        ensure_test_user(&pool, &user_1).await;
        ensure_test_user(&pool, &user_2).await;

        storage.add_member(&room_id, &user_0, "join", None, None, None, None).await.unwrap();
        storage.add_member(&room_id, &user_1, "join", None, None, None, None).await.unwrap();
        storage.add_member(&room_id, &user_2, "join", None, None, None, None).await.unwrap();

        // No cursor, limit=2 — first page
        let page = storage.get_room_members_paginated(&room_id, "join", 2, None).await.unwrap();
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].user_id, user_0);
        assert_eq!(page[1].user_id, user_1);

        // Cursor from user_0, limit=2 — second page
        let page = storage.get_room_members_paginated(&room_id, "join", 2, Some(&user_0)).await.unwrap();
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].user_id, user_1);
        assert_eq!(page[1].user_id, user_2);

        // Cursor from user_1, limit=5 — remaining
        let page = storage.get_room_members_paginated(&room_id, "join", 5, Some(&user_1)).await.unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].user_id, user_2);

        cleanup_membership_data(&pool, &suffix).await;
    }

    // ── 7. remove_member ──────────────────────────────────────────

    #[tokio::test]
    async fn test_remove_member() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mem_remove_{suffix}:localhost");
        let room_id = format!("!room_remove_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;

        storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();
        assert!(storage.is_member(&room_id, &user_id).await.unwrap());

        storage.remove_member(&room_id, &user_id).await.unwrap();

        // Should no longer be a member
        assert!(!storage.is_member(&room_id, &user_id).await.unwrap());
        let m = storage.get_member(&room_id, &user_id).await.unwrap();
        assert!(m.is_some());
        assert_eq!(m.unwrap().membership, "leave");

        cleanup_membership_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_remove_member_idempotent() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mem_rm_idem_{suffix}:localhost");
        let room_id = format!("!room_rm_idem_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;

        storage.add_member(&room_id, &user_id, "leave", None, None, None, None).await.unwrap();
        // Removing an already-left member should not panic/error
        let result = storage.remove_member(&room_id, &user_id).await;
        assert!(result.is_ok());

        cleanup_membership_data(&pool, &suffix).await;
    }

    // ── 8. forget_member / is_forgotten ────────────────────────────

    #[tokio::test]
    async fn test_forget_member() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mem_forget_{suffix}:localhost");
        let room_id = format!("!room_forget_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;

        // Join then leave, then forget
        storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();
        storage.remove_member(&room_id, &user_id).await.unwrap();

        storage.forget_member(&room_id, &user_id).await.unwrap();

        assert!(storage.is_forgotten(&room_id, &user_id).await.unwrap());

        cleanup_membership_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_is_forgotten_returns_false() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mem_not_fgt_{suffix}:localhost");
        let room_id = format!("!room_not_fgt_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;

        storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();

        // Not forgotten (still join)
        assert!(!storage.is_forgotten(&room_id, &user_id).await.unwrap());

        // Non-existent member
        assert!(!storage.is_forgotten(&room_id, "@noone:localhost").await.unwrap());

        cleanup_membership_data(&pool, &suffix).await;
    }

    // ── 9. get_shared_room_users ───────────────────────────────────

    #[tokio::test]
    async fn test_get_shared_room_users_shared() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_a = format!("@mem_share_a_{suffix}:localhost");
        let user_b = format!("@mem_share_b_{suffix}:localhost");
        let room_id = format!("!room_share_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_a).await;
        ensure_test_user(&pool, &user_b).await;

        storage.add_member(&room_id, &user_a, "join", None, None, None, None).await.unwrap();
        storage.add_member(&room_id, &user_b, "join", None, None, None, None).await.unwrap();

        let shared = storage.get_shared_room_users(&user_a).await.unwrap();
        assert_eq!(shared.len(), 1);
        assert!(shared.contains(&user_b));

        cleanup_membership_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_shared_room_users_no_shared() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_a = format!("@mem_noshare_{suffix}:localhost");
        let room_id = format!("!room_noshare_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_a).await;

        storage.add_member(&room_id, &user_a, "join", None, None, None, None).await.unwrap();

        let shared = storage.get_shared_room_users(&user_a).await.unwrap();
        assert!(shared.is_empty());

        cleanup_membership_data(&pool, &suffix).await;
    }

    // ── 10. remove_all_members ─────────────────────────────────────

    #[tokio::test]
    async fn test_remove_all_members() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_a = format!("@mem_clr_a_{suffix}:localhost");
        let user_b = format!("@mem_clr_b_{suffix}:localhost");
        let room_id = format!("!room_clr_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_a).await;
        ensure_test_user(&pool, &user_b).await;

        storage.add_member(&room_id, &user_a, "join", None, None, None, None).await.unwrap();
        storage.add_member(&room_id, &user_b, "join", None, None, None, None).await.unwrap();

        let count_before = storage.get_room_member_count(&room_id).await.unwrap();
        assert_eq!(count_before, 2);

        storage.remove_all_members(&room_id).await.unwrap();

        let members = storage.get_room_members(&room_id, "join").await.unwrap();
        assert!(members.is_empty());

        // Idempotent
        let result = storage.remove_all_members(&room_id).await;
        assert!(result.is_ok());

        cleanup_membership_data(&pool, &suffix).await;
    }

    // ── 11. ban_member / unban_member ──────────────────────────────

    #[tokio::test]
    async fn test_ban_and_unban_member() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mem_ban_{suffix}:localhost");
        let banned_by = format!("@admin_{suffix}:localhost");
        let room_id = format!("!room_ban_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;
        ensure_test_user(&pool, &banned_by).await;

        // Ban (works even without prior add_member)
        storage.ban_member(&room_id, &user_id, &banned_by).await.unwrap();

        let m = storage.get_member(&room_id, &user_id).await.unwrap().unwrap();
        assert_eq!(m.membership, "ban");
        assert_eq!(m.banned_by.as_deref(), Some(banned_by.as_str()));

        // Unban
        storage.unban_member(&room_id, &user_id).await.unwrap();

        let m = storage.get_member(&room_id, &user_id).await.unwrap().unwrap();
        assert_eq!(m.membership, "leave");
        assert!(m.banned_by.is_none());

        cleanup_membership_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_ban_member_idempotent() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mem_ban2_{suffix}:localhost");
        let admin = format!("@admin2_{suffix}:localhost");
        let room_id = format!("!room_ban2_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;
        ensure_test_user(&pool, &admin).await;

        storage.ban_member(&room_id, &user_id, &admin).await.unwrap();
        // Banning again should not fail
        let result = storage.ban_member(&room_id, &user_id, &admin).await;
        assert!(result.is_ok());

        cleanup_membership_data(&pool, &suffix).await;
    }

    // ── 12. get_joined_rooms ───────────────────────────────────────

    #[tokio::test]
    async fn test_get_joined_rooms_multiple() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mem_jrooms_{suffix}:localhost");
        let room_1 = format!("!room_jr1_{suffix}:localhost");
        let room_2 = format!("!room_jr2_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_1).await;
        ensure_test_room(&pool, &room_2).await;
        ensure_test_user(&pool, &user_id).await;

        storage.add_member(&room_1, &user_id, "join", None, None, None, None).await.unwrap();
        storage.add_member(&room_2, &user_id, "join", None, None, None, None).await.unwrap();

        let rooms = storage.get_joined_rooms(&user_id).await.unwrap();
        assert_eq!(rooms.len(), 2);
        assert!(rooms.contains(&room_1));
        assert!(rooms.contains(&room_2));

        cleanup_membership_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_joined_rooms_empty() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mem_nojr_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let rooms = storage.get_joined_rooms(&user_id).await.unwrap();
        assert!(rooms.is_empty());

        cleanup_membership_data(&pool, &suffix).await;
    }

    // ── 13. get_sync_rooms ─────────────────────────────────────────

    #[tokio::test]
    async fn test_get_sync_rooms() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mem_sync_{suffix}:localhost");
        let room_1 = format!("!room_sync1_{suffix}:localhost");
        let room_2 = format!("!room_sync2_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_1).await;
        ensure_test_room(&pool, &room_2).await;
        ensure_test_user(&pool, &user_id).await;

        storage.add_member(&room_1, &user_id, "join", None, None, None, None).await.unwrap();
        storage.add_member(&room_2, &user_id, "join", None, None, None, None).await.unwrap();
        // Leave room_2
        storage.remove_member(&room_2, &user_id).await.unwrap();

        // Without leave
        let rooms = storage.get_sync_rooms(&user_id, false).await.unwrap();
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].membership, "join");

        // With leave
        let rooms = storage.get_sync_rooms(&user_id, true).await.unwrap();
        assert_eq!(rooms.len(), 2);
        let memberships: Vec<&str> = rooms.iter().map(|r| r.membership.as_str()).collect();
        assert!(memberships.contains(&"join"));
        assert!(memberships.contains(&"leave"));

        cleanup_membership_data(&pool, &suffix).await;
    }

    // ── 14. get_membership_state ───────────────────────────────────

    #[tokio::test]
    async fn test_get_membership_state() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mem_state_{suffix}:localhost");
        let room_id = format!("!room_state_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;

        // No membership
        let state = storage.get_membership_state(&room_id, &user_id).await.unwrap();
        assert!(state.is_none());

        // Join
        storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();
        let state = storage.get_membership_state(&room_id, &user_id).await.unwrap();
        assert_eq!(state.as_deref(), Some("join"));

        // Leave
        storage.remove_member(&room_id, &user_id).await.unwrap();
        let state = storage.get_membership_state(&room_id, &user_id).await.unwrap();
        assert_eq!(state.as_deref(), Some("leave"));

        cleanup_membership_data(&pool, &suffix).await;
    }

    // ── 15. get_joined_room_count ──────────────────────────────────

    #[tokio::test]
    async fn test_get_joined_room_count() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mem_jrcount_{suffix}:localhost");
        let room_a = format!("!room_jrc_a_{suffix}:localhost");
        let room_b = format!("!room_jrc_b_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_a).await;
        ensure_test_room(&pool, &room_b).await;
        ensure_test_user(&pool, &user_id).await;

        // Zero
        let count = storage.get_joined_room_count(&user_id).await.unwrap();
        assert_eq!(count, 0);

        // With rooms
        storage.add_member(&room_a, &user_id, "join", None, None, None, None).await.unwrap();
        storage.add_member(&room_b, &user_id, "join", None, None, None, None).await.unwrap();
        let count = storage.get_joined_room_count(&user_id).await.unwrap();
        assert_eq!(count, 2);

        cleanup_membership_data(&pool, &suffix).await;
    }

    // ── 16. is_member ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_is_member() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mem_ismem_{suffix}:localhost");
        let room_id = format!("!room_ismem_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;

        // Not a member yet
        assert!(!storage.is_member(&room_id, &user_id).await.unwrap());

        // Join
        storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();
        assert!(storage.is_member(&room_id, &user_id).await.unwrap());

        // Leave
        storage.remove_member(&room_id, &user_id).await.unwrap();
        assert!(!storage.is_member(&room_id, &user_id).await.unwrap());

        cleanup_membership_data(&pool, &suffix).await;
    }

    // ── 17. get_room_member ────────────────────────────────────────

    #[tokio::test]
    async fn test_get_room_member() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mem_rmem_{suffix}:localhost");
        let room_id = format!("!room_rmem_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;

        // Not found
        let m = storage.get_room_member(&room_id, &user_id).await.unwrap();
        assert!(m.is_none());

        // Found
        storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();
        let m = storage.get_room_member(&room_id, &user_id).await.unwrap();
        assert!(m.is_some());
        assert_eq!(m.unwrap().membership, "join");

        cleanup_membership_data(&pool, &suffix).await;
    }

    // ── 18. get_joined_members ─────────────────────────────────────

    #[tokio::test]
    async fn test_get_joined_members() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_a = format!("@mem_jm_a_{suffix}:localhost");
        let user_b = format!("@mem_jm_b_{suffix}:localhost");
        let room_id = format!("!room_jm_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_a).await;
        ensure_test_user(&pool, &user_b).await;

        // Join only
        storage.add_member(&room_id, &user_a, "join", None, None, None, None).await.unwrap();
        // Invite (should NOT be in joined members)
        storage.add_member(&room_id, &user_b, "invite", None, None, None, None).await.unwrap();

        let joined = storage.get_joined_members(&room_id).await.unwrap();
        assert_eq!(joined.len(), 1);
        assert_eq!(joined[0].user_id, user_a);

        cleanup_membership_data(&pool, &suffix).await;
    }

    // ── 19. get_joined_member ──────────────────────────────────────

    #[tokio::test]
    async fn test_get_joined_member() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_id = format!("@mem_jm_one_{suffix}:localhost");
        let room_id = format!("!room_jm_one_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_id).await;

        storage.add_member(&room_id, &user_id, "join", None, None, None, None).await.unwrap();

        // Found for join
        let m = storage.get_joined_member(&room_id, &user_id).await.unwrap();
        assert!(m.is_some());
        assert_eq!(m.unwrap().membership, "join");

        // Leave the room
        storage.remove_member(&room_id, &user_id).await.unwrap();

        // Not found for leave
        let m = storage.get_joined_member(&room_id, &user_id).await.unwrap();
        assert!(m.is_none());

        cleanup_membership_data(&pool, &suffix).await;
    }

    // ── 20. share_common_room ──────────────────────────────────────

    #[tokio::test]
    async fn test_share_common_room() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_a = format!("@mem_scr_a_{suffix}:localhost");
        let user_b = format!("@mem_scr_b_{suffix}:localhost");
        let user_c = format!("@mem_scr_c_{suffix}:localhost");
        let room_id = format!("!room_scr_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_a).await;
        ensure_test_user(&pool, &user_b).await;
        ensure_test_user(&pool, &user_c).await;

        storage.add_member(&room_id, &user_a, "join", None, None, None, None).await.unwrap();
        storage.add_member(&room_id, &user_b, "join", None, None, None, None).await.unwrap();

        // Shares common room
        assert!(storage.share_common_room(&user_a, &user_b).await.unwrap());

        // Does not share (user_c not in any room)
        assert!(!storage.share_common_room(&user_a, &user_c).await.unwrap());

        cleanup_membership_data(&pool, &suffix).await;
    }

    // ── 21. get_membership_history ─────────────────────────────────

    #[tokio::test]
    async fn test_get_membership_history() {
        let pool = test_pool().await;
        let storage = RoomMemberStorage::new(&pool, "localhost");
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        let user_a = format!("@mem_hist_a_{suffix}:localhost");
        let user_b = format!("@mem_hist_b_{suffix}:localhost");
        let room_id = format!("!room_hist_{suffix}:localhost");

        cleanup_membership_data(&pool, &suffix).await;
        ensure_test_room(&pool, &room_id).await;
        ensure_test_user(&pool, &user_a).await;
        ensure_test_user(&pool, &user_b).await;

        storage.add_member(&room_id, &user_a, "join", None, None, None, None).await.unwrap();
        storage.add_member(&room_id, &user_b, "invite", None, None, None, None).await.unwrap();

        let history = storage.get_membership_history(&room_id, 10).await.unwrap();
        assert_eq!(history.len(), 2);
        // Ordered by updated_ts DESC — most recent first
        assert_eq!(history[0].user_id, user_b);
        assert_eq!(history[1].user_id, user_a);

        // Limit
        let limited = storage.get_membership_history(&room_id, 1).await.unwrap();
        assert_eq!(limited.len(), 1);
        assert_eq!(limited[0].user_id, user_b);

        cleanup_membership_data(&pool, &suffix).await;
    }
}

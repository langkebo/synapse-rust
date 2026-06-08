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
        // Use explicit sender if provided (e.g. inviter), otherwise default to user_id
        let effective_sender = sender.unwrap_or(user_id);

        let q = sqlx::query_as!(RoomMember,
            r#"
            INSERT INTO room_memberships (room_id, user_id, sender, membership, event_id, event_type, display_name, join_reason, updated_ts, joined_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (room_id, user_id) DO UPDATE SET
                display_name = EXCLUDED.display_name,
                membership = EXCLUDED.membership,
                join_reason = EXCLUDED.join_reason,
                updated_ts = EXCLUDED.updated_ts
            RETURNING room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            "#,
            room_id,
            user_id,
            effective_sender,
            membership,
            event_id,
            "m.room.member",
            display_name,
            join_reason,
            now,
            now
        );

        if let Some(tx) = tx {
            q.fetch_one(&mut **tx).await
        } else {
            q.fetch_one(&*self.pool).await
        }
    }

    pub async fn get_member(&self, room_id: &str, user_id: &str) -> Result<Option<RoomMember>, sqlx::Error> {
        sqlx::query_as!(
            RoomMember,
            r#"SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            FROM room_memberships WHERE room_id = $1 AND user_id = $2"#,
            room_id,
            user_id
        )
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_room_members(&self, room_id: &str, membership_type: &str) -> Result<Vec<RoomMember>, sqlx::Error> {
        sqlx::query_as!(
            RoomMember,
            r#"SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            FROM room_memberships WHERE room_id = $1 AND membership = $2"#,
            room_id,
            membership_type
        )
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_room_member_count(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar!(
            r#"SELECT COALESCE(COUNT(*), 0)::BIGINT AS "count!" FROM room_memberships WHERE room_id = $1 AND membership = 'join'"#,
            room_id
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
    }

    pub async fn remove_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query!(
            r#"UPDATE room_memberships SET membership = 'leave', left_ts = $3, updated_ts = $3, is_banned = false WHERE room_id = $1 AND user_id = $2 AND membership IN ('join', 'ban')"#,
            room_id,
            user_id,
            now
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn forget_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query!(
            r#"UPDATE room_memberships SET membership = 'forget', left_ts = $3, updated_ts = $3 WHERE room_id = $1 AND user_id = $2 AND membership IN ('leave', 'invite')"#,
            room_id,
            user_id,
            now
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn is_forgotten(&self, room_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r#"SELECT 1 AS "exists!: i32" FROM room_memberships WHERE room_id = $1 AND user_id = $2 AND membership = 'forget' LIMIT 1"#,
            room_id,
            user_id
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.is_some())
    }

    pub async fn get_shared_room_users(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar!(
            r#"SELECT DISTINCT m2.user_id
            FROM room_memberships m1
            JOIN room_memberships m2 ON m1.room_id = m2.room_id
            WHERE m1.user_id = $1 AND m1.membership = 'join'
              AND m2.membership = 'join'
              AND m2.user_id != $1"#,
            user_id
        )
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn remove_all_members(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!("DELETE FROM room_memberships WHERE room_id = $1", room_id)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    pub async fn ban_member(&self, room_id: &str, user_id: &str, banned_by: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"INSERT INTO room_memberships (room_id, user_id, membership, banned_by)
            VALUES ($1, $2, 'ban', $3)
            ON CONFLICT (room_id, user_id) DO UPDATE SET
                membership = 'ban',
                banned_by = EXCLUDED.banned_by"#,
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
            r#"UPDATE room_memberships SET membership = 'leave', banned_by = NULL WHERE room_id = $1 AND user_id = $2"#,
            room_id,
            user_id
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_joined_rooms(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar!(
            r#"SELECT room_id FROM room_memberships WHERE user_id = $1 AND membership = 'join'"#,
            user_id
        )
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_sync_rooms(
        &self,
        user_id: &str,
        include_leave: bool,
    ) -> Result<Vec<UserRoomMembership>, sqlx::Error> {
        if include_leave {
            sqlx::query_as!(
                UserRoomMembership,
                r#"SELECT room_id, membership FROM room_memberships WHERE user_id = $1 AND membership IN ('join', 'leave') ORDER BY updated_ts DESC NULLS LAST, room_id ASC"#,
                user_id
            )
            .fetch_all(&*self.pool)
            .await
        } else {
            sqlx::query_as!(
                UserRoomMembership,
                r#"SELECT room_id, membership FROM room_memberships WHERE user_id = $1 AND membership = 'join' ORDER BY updated_ts DESC NULLS LAST, room_id ASC"#,
                user_id
            )
            .fetch_all(&*self.pool)
            .await
        }
    }

    pub async fn get_membership_state(&self, room_id: &str, user_id: &str) -> Result<Option<String>, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r#"SELECT membership FROM room_memberships WHERE room_id = $1 AND user_id = $2"#,
            room_id,
            user_id
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result)
    }

    pub async fn get_joined_room_count(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar!(
            r#"SELECT COUNT(*)::BIGINT AS "count!" FROM room_memberships WHERE user_id = $1 AND membership = 'join'"#,
            user_id
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
    }

    pub async fn is_member(&self, room_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r#"SELECT 1 AS "exists!: i32" FROM room_memberships WHERE room_id = $1 AND user_id = $2 AND membership = 'join' LIMIT 1"#,
            room_id,
            user_id
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.is_some())
    }

    pub async fn get_room_member(&self, room_id: &str, user_id: &str) -> Result<Option<RoomMember>, sqlx::Error> {
        sqlx::query_as!(
            RoomMember,
            r#"SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            FROM room_memberships WHERE room_id = $1 AND user_id = $2"#,
            room_id,
            user_id
        )
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_joined_members(&self, room_id: &str) -> Result<Vec<RoomMember>, sqlx::Error> {
        sqlx::query_as!(
            RoomMember,
            r#"SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            FROM room_memberships WHERE room_id = $1 AND membership = 'join'"#,
            room_id
        )
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_joined_member(&self, room_id: &str, user_id: &str) -> Result<Option<RoomMember>, sqlx::Error> {
        sqlx::query_as!(
            RoomMember,
            r#"SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            FROM room_memberships WHERE room_id = $1 AND user_id = $2 AND membership = 'join'"#,
            room_id,
            user_id
        )
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn share_common_room(&self, user_id_1: &str, user_id_2: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r#"SELECT 1 AS "exists!: i32" FROM room_memberships m1
            JOIN room_memberships m2 ON m1.room_id = m2.room_id
            WHERE m1.user_id = $1 AND m1.membership = 'join'
              AND m2.user_id = $2 AND m2.membership = 'join'
            LIMIT 1"#,
            user_id_1,
            user_id_2
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.is_some())
    }

    pub async fn get_membership_history(&self, room_id: &str, limit: i64) -> Result<Vec<RoomMember>, sqlx::Error> {
        sqlx::query_as!(
            RoomMember,
            r#"SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            FROM room_memberships WHERE room_id = $1
            ORDER BY updated_ts DESC
            LIMIT $2"#,
            room_id,
            limit
        )
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_joined_rooms_with_details(
        &self,
        user_id: &str,
    ) -> Result<Vec<(String, String, Option<String>, Option<String>)>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT r.room_id, r.name, r.topic, r.avatar_url
            FROM room_memberships rm
            JOIN rooms r ON rm.room_id = r.room_id
            WHERE rm.user_id = $1 AND rm.membership = 'join'
            ORDER BY r.created_ts DESC
            "#,
            user_id
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| (r.room_id, r.name.unwrap_or_default(), r.topic, r.avatar_url)).collect())
    }

    pub async fn get_room_members_with_profiles(
        &self,
        room_id: &str,
        membership_type: &str,
    ) -> Result<Vec<(RoomMember, Option<String>, Option<String>)>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT rm.room_id as "room_id!", rm.user_id as "user_id!", rm.sender, rm.membership as "membership!", rm.event_id, rm.event_type,
                   rm.display_name, rm.avatar_url, rm.is_banned, rm.invite_token, rm.updated_ts,
                   rm.joined_ts, rm.left_ts, rm.reason, rm.banned_by, rm.ban_reason, rm.banned_ts, rm.join_reason,
                   u.displayname as user_displayname, u.avatar_url as user_avatar_url
            FROM room_memberships rm
            LEFT JOIN users u ON rm.user_id = u.user_id
            WHERE rm.room_id = $1 AND rm.membership = $2
            "#,
            room_id,
            membership_type
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|row| {
                let member = RoomMember {
                    room_id: row.room_id.clone(),
                    user_id: row.user_id.clone(),
                    sender: row.sender.clone(),
                    membership: row.membership.clone(),
                    event_id: row.event_id.clone(),
                    event_type: row.event_type.clone(),
                    display_name: row.display_name.clone(),
                    avatar_url: row.avatar_url.clone(),
                    is_banned: row.is_banned,
                    invite_token: row.invite_token.clone(),
                    updated_ts: row.updated_ts,
                    joined_ts: row.joined_ts,
                    left_ts: row.left_ts,
                    reason: row.reason.clone(),
                    banned_by: row.banned_by.clone(),
                    ban_reason: row.ban_reason.clone(),
                    banned_ts: row.banned_ts,
                    join_reason: row.join_reason.clone(),
                };
                (member, row.user_displayname.clone(), row.user_avatar_url.clone())
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

        let rows = sqlx::query_as!(
            RoomMember,
            r#"SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            FROM room_memberships
            WHERE room_id = ANY($1) AND membership = $2"#,
            room_ids,
            membership_type
        )
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

        let rows = sqlx::query_scalar!(
            r#"SELECT user_id FROM room_memberships
            WHERE room_id = $1 AND user_id = ANY($2) AND membership = $3"#,
            room_id,
            user_ids,
            membership_type
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().collect())
    }

    pub async fn set_ban_reason(&self, room_id: &str, user_id: &str, reason: &str) -> Result<(), sqlx::Error> {
        sqlx::query!("UPDATE room_memberships SET ban_reason = $3 WHERE room_id = $1 AND user_id = $2", room_id, user_id, reason)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    pub async fn force_leave_membership(&self, room_id: &str, user_id: &str, now: i64) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE room_memberships SET membership = 'leave', left_ts = $3, updated_ts = $3 WHERE room_id = $1 AND user_id = $2"#,
            room_id,
            user_id,
            now
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
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

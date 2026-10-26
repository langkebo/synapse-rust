/// The `api` module.
pub(crate) mod api;

pub use api::MemberStoreApi;

use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

/// Count the joined members of a room.
///
/// Shared cross-layer helper: `room_memberships` is owned by this module, but the
/// joined-member count is also needed by `RoomStorage::get_single_room_stats` and
/// `BeaconStorage::get_joined_member_count`. Keeping the single copy of the SQL
/// here stops the three copies from drifting apart.
pub(crate) async fn count_joined_room_members(pool: &Pool<Postgres>, room_id: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"
            SELECT COALESCE(COUNT(*), 0) AS "count!" FROM room_memberships WHERE room_id = $1 AND membership = 'join'
            "#,
        room_id,
    )
    .fetch_one(pool)
    .await
}

/// The `RoomMember` struct.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct RoomMember {
    /// The `room_id` field.
    pub room_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `sender` field.
    pub sender: Option<String>,
    /// The `membership` field.
    pub membership: String,
    /// The `event_id` field.
    pub event_id: Option<String>,
    /// The `event_type` field.
    pub event_type: Option<String>,
    /// The `display_name` field.
    pub display_name: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `is_banned` field.
    pub is_banned: Option<bool>,
    /// The `invite_token` field.
    pub invite_token: Option<String>,
    /// The `updated_ts` field.
    pub updated_ts: Option<i64>,
    /// The `joined_ts` field.
    pub joined_ts: Option<i64>,
    /// The `left_ts` field.
    pub left_ts: Option<i64>,
    /// The `reason` field.
    pub reason: Option<String>,
    /// The `banned_by` field.
    pub banned_by: Option<String>,
    /// The `ban_reason` field.
    pub ban_reason: Option<String>,
    /// The `banned_ts` field.
    pub banned_ts: Option<i64>,
    /// The `join_reason` field.
    pub join_reason: Option<String>,
}

/// The `UserRoomMembership` struct.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserRoomMembership {
    /// The `room_id` field.
    pub room_id: String,
    /// The `membership` field.
    pub membership: String,
}

/// The `RoomMemberStorage` struct.
#[derive(Clone)]
pub struct RoomMemberStorage {
    /// The `pool` field.
    pub pool: Arc<Pool<Postgres>>,
    /// This storage's local Matrix server name (assembly information).
    ///
    /// It is **not** used to mint event IDs: membership event identity comes from
    /// the write entry that persists the event (decision §4.1).
    pub server_name: String,
}

impl RoomMemberStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<Pool<Postgres>>, server_name: &str) -> Self {
        Self { pool: pool.clone(), server_name: server_name.to_string() }
    }

    /// See [`add_member`].
    // reason: `add_member` 是协议/存储字段的直传签名：参数逐个来自调用点，打包成结构体只会把同一份字段列表抄两遍
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
        let now = current_timestamp_millis();
        // Decision §4.1: the write entry owns event identity, and `add_member`
        // writes the `room_memberships` **projection**, not the membership event
        // itself, so it cannot know that event's final ID. Minting one here
        // produced a `$$…` value that (a) is not a valid Matrix event ID shape and
        // (b) is guaranteed to disagree with the `events` row for every v3+ room
        // — two identities for one event. The column is left NULL: the `events`
        // row is the single source of the ID, and `Option<RoomMember::event_id>`
        // already models "no ID recorded" for the paths that never had one.
        let event_id: Option<String> = None;
        let joined_ts = if membership == "join" { Some(now) } else { None };
        // Use explicit sender if provided (e.g. inviter), otherwise default to user_id
        let effective_sender = sender.unwrap_or(user_id);

        let query = sqlx::query_as!(
            RoomMember,
            r"
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
            ",
            room_id,
            user_id,
            effective_sender,
            membership,
            event_id,
            "m.room.member",
            display_name,
            join_reason,
            now,
            joined_ts,
        );

        if let Some(tx) = tx {
            query.fetch_one(&mut **tx).await
        } else {
            query.fetch_one(&*self.pool).await
        }
    }

    /// See [`get_member`].
    pub async fn get_member(&self, room_id: &str, user_id: &str) -> Result<Option<RoomMember>, sqlx::Error> {
        sqlx::query_as!(
            RoomMember,
            r"
            SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            FROM room_memberships WHERE room_id = $1 AND user_id = $2
            ",
            room_id,
            user_id,
        )
        .fetch_optional(&*self.pool)
        .await
    }

    /// See [`get_room_members`].
    pub async fn get_room_members(&self, room_id: &str, membership_type: &str) -> Result<Vec<RoomMember>, sqlx::Error> {
        sqlx::query_as!(
            RoomMember,
            r"
            SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            FROM room_memberships WHERE room_id = $1 AND membership = $2
            ",
            room_id,
            membership_type,
        )
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
        let exists = sqlx::query_scalar!(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM room_memberships
                WHERE room_id = $1
                  AND user_id LIKE $2
                  AND membership IN ('join', 'invite', 'leave')
            ) AS "exists!"
            "#,
            room_id,
            &domain_pattern,
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(exists)
    }

    /// See [`get_room_member_count`].
    pub async fn get_room_member_count(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        count_joined_room_members(&self.pool, room_id).await
    }

    /// See [`get_room_members_paginated`].
    pub async fn get_room_members_paginated(
        &self,
        room_id: &str,
        membership_type: &str,
        limit: i64,
        from_user_id: Option<&str>,
    ) -> Result<Vec<RoomMember>, sqlx::Error> {
        if let Some(from_user_id) = from_user_id {
            sqlx::query_as!(
                RoomMember,
                r"
                SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
                FROM room_memberships
                WHERE room_id = $1 AND membership = $2 AND user_id > $3
                ORDER BY user_id ASC
                LIMIT $4
                ",
                room_id,
                membership_type,
                from_user_id,
                limit,
            )
            .fetch_all(&*self.pool)
            .await
        } else {
            sqlx::query_as!(
                RoomMember,
                r"
                SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
                FROM room_memberships
                WHERE room_id = $1 AND membership = $2
                ORDER BY user_id ASC
                LIMIT $3
                ",
                room_id,
                membership_type,
                limit,
            )
            .fetch_all(&*self.pool)
            .await
        }
    }

    /// Remove (leave) a member. When `tx` is provided the UPDATE runs in the
    /// caller's transaction (used by MSC4267 leave+forget to bundle the
    /// leave+forget mutations atomically). When `tx` is `None` the existing
    /// pool-backed path is used.
    pub async fn remove_member(
        &self,
        room_id: &str,
        user_id: &str,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        let query = sqlx::query!(
            r"
            UPDATE room_memberships
            SET membership = 'leave',
                left_ts = $3,
                updated_ts = $3,
                is_banned = false
            WHERE room_id = $1 AND user_id = $2 AND membership IN ('join', 'ban', 'invite')
            ",
            room_id,
            user_id,
            now,
        );
        if let Some(tx) = tx {
            query.execute(&mut **tx).await?;
        } else {
            query.execute(&*self.pool).await?;
        }
        Ok(())
    }

    /// Mark a membership as 'forget'. When `tx` is provided the UPDATE runs
    /// in the caller's transaction (used by MSC4267 leave+forget).
    pub async fn forget_member(
        &self,
        room_id: &str,
        user_id: &str,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        let query = sqlx::query!(
            r"
            UPDATE room_memberships
            SET membership = 'forget',
                left_ts = $3,
                updated_ts = $3
            WHERE room_id = $1 AND user_id = $2 AND membership IN ('leave', 'invite')
            ",
            room_id,
            user_id,
            now,
        );
        if let Some(tx) = tx {
            query.execute(&mut **tx).await?;
        } else {
            query.execute(&*self.pool).await?;
        }
        Ok(())
    }

    /// See [`is_forgotten`].
    pub async fn is_forgotten(&self, room_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r#"
            SELECT 1 AS "exists" FROM room_memberships
            WHERE room_id = $1 AND user_id = $2 AND membership = 'forget'
            LIMIT 1
            "#,
            room_id,
            user_id,
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.is_some())
    }

    /// See [`get_shared_room_users`].
    pub async fn get_shared_room_users(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar!(
            r"
            SELECT DISTINCT m2.user_id
            FROM room_memberships m1
            JOIN room_memberships m2 ON m1.room_id = m2.room_id
            WHERE m1.user_id = $1 AND m1.membership = 'join'
              AND m2.membership = 'join'
              AND m2.user_id != $1
            ",
            user_id,
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`MemberStoreApi::get_users_no_longer_sharing_rooms_since`].
    ///
    /// MSC4262 `LEFT_ROOM` (upstream #20203). Two directions are covered:
    ///
    /// * **(A)** someone left/banned from a room the requester is still joined
    ///   to (`membership <> 'join'` with `updated_ts > since_ts`);
    /// * **(B)** the requester themself left a room — everyone who had already
    ///   joined that room before the departure no longer shares a room with
    ///   them.
    ///
    /// Users the requester still shares *any* joined room with are excluded.
    /// `joined_ts IS NOT NULL` filters out invite-only memberships that never
    /// became a joined room, so we never leak an ID that never shared a room.
    ///
    /// `updated_ts` is the change signal: `add_member`, `remove_member`,
    /// `forget_member`, `force_leave_membership` and `ban_member` all write it.
    pub async fn get_users_no_longer_sharing_rooms_since(
        &self,
        user_id: &str,
        since_ts: i64,
    ) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar!(
            r"
            WITH my_changes AS (
                SELECT room_id, membership, COALESCE(updated_ts, 0) AS changed_ts
                FROM room_memberships
                WHERE user_id = $1 AND COALESCE(updated_ts, 0) > $2
            ),
            departed_rooms AS (
                SELECT room_id, changed_ts FROM my_changes WHERE membership <> 'join'
            ),
            joined_rooms AS (
                SELECT room_id
                FROM room_memberships
                WHERE user_id = $1 AND membership = 'join'
            )
            SELECT DISTINCT other.user_id
            FROM room_memberships other
            WHERE other.user_id <> $1
              AND other.membership IN ('join', 'leave', 'ban')
              AND other.joined_ts IS NOT NULL
              AND (
                  (
                      other.room_id IN (SELECT room_id FROM joined_rooms)
                      AND other.membership <> 'join'
                      AND COALESCE(other.updated_ts, 0) > $2
                  )
                  OR EXISTS (
                      SELECT 1 FROM departed_rooms d
                      WHERE d.room_id = other.room_id
                        AND other.joined_ts < d.changed_ts
                  )
              )
              AND NOT EXISTS (
                  SELECT 1
                  FROM room_memberships mine
                  JOIN room_memberships theirs ON mine.room_id = theirs.room_id
                  WHERE mine.user_id = $1 AND mine.membership = 'join'
                    AND theirs.user_id = other.user_id AND theirs.membership = 'join'
              )
            ",
            user_id,
            since_ts,
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`remove_all_members`].
    pub async fn remove_all_members(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            DELETE FROM room_memberships WHERE room_id = $1
            ",
            room_id,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`ban_member`].
    pub async fn ban_member(&self, room_id: &str, user_id: &str, banned_by: &str) -> Result<(), sqlx::Error> {
        // `updated_ts` 是必需的：MSC4262 的 LEFT_ROOM 窗口靠它判断"何时不再共享房间"，
        // 缺了它 ban 不会从对端视图里消失。走 `query!` 让新增列在编译期被校验
        // （早前为免 `.sqlx` 重生成写成运行期查询，代价是这条语句脱离校验面）。
        let now = current_timestamp_millis();
        sqlx::query!(
            r"
            INSERT INTO room_memberships (room_id, user_id, membership, banned_by, updated_ts)
            VALUES ($1, $2, 'ban', $3, $4)
            ON CONFLICT (room_id, user_id) DO UPDATE SET
                membership = 'ban',
                banned_by = EXCLUDED.banned_by,
                updated_ts = EXCLUDED.updated_ts
            ",
            room_id,
            user_id,
            banned_by,
            now,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`unban_member`].
    pub async fn unban_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        let result = sqlx::query!(
            r"
            UPDATE room_memberships SET membership = 'leave', banned_by = NULL
            WHERE room_id = $1 AND user_id = $2 AND membership = 'ban'
            ",
            room_id,
            user_id,
        )
        .execute(&*self.pool)
        .await?;

        // An unban ends the ban, so its reason must not outlive it — otherwise a
        // later `ban` without a reason would inherit the previous one. Only
        // clear when the UPDATE actually lifted a ban.
        if result.rows_affected() > 0 {
            self.set_ban_reason(room_id, user_id, None).await?;
        }
        Ok(())
    }

    /// See [`get_joined_rooms`].
    pub async fn get_joined_rooms(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query_scalar!(
            r"
            SELECT room_id FROM room_memberships WHERE user_id = $1 AND membership = 'join'
            ",
            user_id,
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// Keyset-paginated variant: returns at most `limit` rooms with `room_id > after_room_id`.
    /// This avoids `OFFSET` performance degradation on large membership sets.
    pub async fn get_joined_rooms_page(
        &self,
        user_id: &str,
        after_room_id: &str,
        limit: i64,
    ) -> Result<Vec<String>, sqlx::Error> {
        if limit <= 0 {
            return Ok(Vec::new());
        }
        let rows = sqlx::query_scalar!(
            r"
            SELECT room_id
              FROM room_memberships
             WHERE user_id = $1
               AND membership = 'join'
               AND room_id > $2
          ORDER BY room_id
             LIMIT $3
            ",
            user_id,
            after_room_id,
            limit,
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// See [`get_sync_rooms`].
    /// See [`get_sync_rooms`].
    ///
    /// Every membership that owns a `/sync` section must be returned here — this
    /// is the only producer of the list `/sync` buckets into sections
    /// (`sync_service::room_sections_from_memberships` maps `invite` → `rooms.invite`
    /// and `knock` → `rooms.knock`). Filtering a membership out here silently
    /// empties its section: the invitee/knocker renders nothing, and the whole
    /// `invite_state` / `knock_state` pipeline becomes unreachable.
    ///
    /// `ban` is deliberately **not** included yet: the section mapper has no arm
    /// for it, so it would land in the catch-all `rooms.join` and render a banned
    /// user as joined. Adding bans means adding the `rooms.leave` arm first.
    pub async fn get_sync_rooms(
        &self,
        user_id: &str,
        include_leave: bool,
    ) -> Result<Vec<UserRoomMembership>, sqlx::Error> {
        let memberships = if include_leave {
            sqlx::query_as!(
                UserRoomMembership,
                r"
                SELECT room_id, membership
                FROM room_memberships
                WHERE user_id = $1 AND membership IN ('join', 'invite', 'knock', 'leave')
                ORDER BY updated_ts DESC NULLS LAST, room_id ASC
                ",
                user_id,
            )
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query_as!(
                UserRoomMembership,
                r"
                SELECT room_id, membership
                FROM room_memberships
                WHERE user_id = $1 AND membership IN ('join', 'invite', 'knock')
                ORDER BY updated_ts DESC NULLS LAST, room_id ASC
                ",
                user_id,
            )
            .fetch_all(&*self.pool)
            .await?
        };

        Ok(memberships)
    }

    /// See [`get_membership_state`].
    pub async fn get_membership_state(&self, room_id: &str, user_id: &str) -> Result<Option<String>, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r"
            SELECT membership FROM room_memberships WHERE room_id = $1 AND user_id = $2
            ",
            room_id,
            user_id,
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result)
    }

    /// See [`get_joined_room_count`].
    pub async fn get_joined_room_count(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "count!" FROM room_memberships WHERE user_id = $1 AND membership = 'join'
            "#,
            user_id,
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
    }

    /// See [`is_member`].
    pub async fn is_member(&self, room_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r#"
            SELECT 1 AS "exists" FROM room_memberships WHERE room_id = $1 AND user_id = $2 AND membership = 'join' LIMIT 1
            "#,
            room_id,
            user_id,
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.is_some())
    }

    /// See [`get_room_member`].
    pub async fn get_room_member(&self, room_id: &str, user_id: &str) -> Result<Option<RoomMember>, sqlx::Error> {
        let result = sqlx::query_as!(
            RoomMember,
            r"
            SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            FROM room_memberships WHERE room_id = $1 AND user_id = $2
            ",
            room_id,
            user_id,
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result)
    }

    /// Fetch members for a set of users in a single room using a single
    /// `ANY($2)` query, avoiding the N+1 of per-user `get_room_member` calls.
    pub async fn get_room_members_by_user_ids(
        &self,
        room_id: &str,
        user_ids: &[String],
    ) -> Result<std::collections::HashMap<String, RoomMember>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let members = sqlx::query_as!(
            RoomMember,
            r"
            SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            FROM room_memberships WHERE room_id = $1 AND user_id = ANY($2)
            ",
            room_id,
            user_ids,
        )
        .fetch_all(&*self.pool)
        .await?;
        Ok(members.into_iter().map(|m| (m.user_id.clone(), m)).collect())
    }

    /// See [`get_joined_members`].
    pub async fn get_joined_members(&self, room_id: &str) -> Result<Vec<RoomMember>, sqlx::Error> {
        let members = sqlx::query_as!(
            RoomMember,
            r"
            SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            FROM room_memberships WHERE room_id = $1 AND membership = 'join'
            ",
            room_id,
        )
        .fetch_all(&*self.pool)
        .await?;
        Ok(members)
    }

    /// See [`get_joined_member`].
    pub async fn get_joined_member(&self, room_id: &str, user_id: &str) -> Result<Option<RoomMember>, sqlx::Error> {
        let result = sqlx::query_as!(
            RoomMember,
            r"
            SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            FROM room_memberships WHERE room_id = $1 AND user_id = $2 AND membership = 'join'
            ",
            room_id,
            user_id,
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result)
    }

    /// See [`share_common_room`].
    pub async fn share_common_room(&self, user_id_1: &str, user_id_2: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r"
            SELECT 1 FROM room_memberships m1
            JOIN room_memberships m2 ON m1.room_id = m2.room_id
            WHERE m1.user_id = $1 AND m1.membership = 'join'
              AND m2.user_id = $2 AND m2.membership = 'join'
            LIMIT 1
            ",
            user_id_1,
            user_id_2,
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.is_some())
    }

    /// See [`share_common_rooms_batch`].
    pub async fn share_common_rooms_batch(
        &self,
        user_id: &str,
        other_user_ids: &[String],
    ) -> Result<Vec<String>, sqlx::Error> {
        if other_user_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows: Vec<String> = sqlx::query_scalar!(
            r"
            SELECT DISTINCT m2.user_id
            FROM room_memberships m1
            JOIN room_memberships m2 ON m1.room_id = m2.room_id
            WHERE m1.user_id = $1 AND m1.membership = 'join'
              AND m2.user_id = ANY($2) AND m2.membership = 'join'
            ",
            user_id,
            other_user_ids,
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    /// See [`get_membership_history`].
    pub async fn get_membership_history(&self, room_id: &str, limit: i64) -> Result<Vec<RoomMember>, sqlx::Error> {
        let memberships = sqlx::query_as!(
            RoomMember,
            r"
            SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            FROM room_memberships WHERE room_id = $1
            ORDER BY updated_ts DESC
            LIMIT $2
            ",
            room_id,
            limit,
        )
        .fetch_all(&*self.pool)
        .await?;
        Ok(memberships)
    }

    /// See [`get_joined_rooms_with_details`].
    pub async fn get_joined_rooms_with_details(
        &self,
        user_id: &str,
    ) -> Result<Vec<(String, String, Option<String>, Option<String>)>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT r.room_id, r.name AS "name!", r.topic, r.avatar_url
            FROM room_memberships rm
            JOIN rooms r ON rm.room_id = r.room_id
            WHERE rm.user_id = $1 AND rm.membership = 'join'
            ORDER BY r.created_ts DESC
            "#,
            user_id,
        )
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| (r.room_id, r.name, r.topic, r.avatar_url)).collect())
    }

    /// See [`get_room_members_with_profiles`].
    pub async fn get_room_members_with_profiles(
        &self,
        room_id: &str,
        membership_type: &str,
    ) -> Result<Vec<(RoomMember, Option<String>, Option<String>)>, sqlx::Error> {
        let rows = sqlx::query!(
            r"
            SELECT rm.room_id, rm.user_id, rm.sender, rm.membership, rm.event_id, rm.event_type,
                   rm.display_name, rm.avatar_url, rm.is_banned, rm.invite_token, rm.updated_ts,
                   rm.joined_ts, rm.left_ts, rm.reason, rm.banned_by, rm.ban_reason, rm.banned_ts, rm.join_reason,
                   u.displayname as user_displayname, u.avatar_url as user_avatar_url
            FROM room_memberships rm
            LEFT JOIN users u ON rm.user_id = u.user_id
            WHERE rm.room_id = $1 AND rm.membership = $2
            ",
            room_id,
            membership_type,
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let member = RoomMember {
                    room_id: row.room_id,
                    user_id: row.user_id,
                    sender: row.sender,
                    membership: row.membership,
                    event_id: row.event_id,
                    event_type: row.event_type,
                    display_name: row.display_name,
                    avatar_url: row.avatar_url,
                    is_banned: row.is_banned,
                    invite_token: row.invite_token,
                    updated_ts: row.updated_ts,
                    joined_ts: row.joined_ts,
                    left_ts: row.left_ts,
                    reason: row.reason,
                    banned_by: row.banned_by,
                    ban_reason: row.ban_reason,
                    banned_ts: row.banned_ts,
                    join_reason: row.join_reason,
                };
                (member, row.user_displayname, row.user_avatar_url)
            })
            .collect())
    }

    /// MSC4502: Paginated room members with profiles (display_name, avatar_url).
    ///
    /// Extends `get_room_members_paginated` to include user profiles and
    /// optionally filter out a membership type (`not_membership`).
    ///
    /// Pagination uses lexicographic `user_id` ordering, supporting both
    /// forward (`dir=f`) and backward (`dir=b`) directions via SQL `>`.
    /// The returned `user_id` of the last result becomes the `from` cursor.
    ///
    /// Performance: Uses `idx_room_memberships_room_user` for O(log n) cursor.
    pub async fn get_room_members_paginated_with_profiles(
        &self,
        room_id: &str,
        membership_type: &str,
        not_membership: Option<&str>,
        limit: i64,
        from_user_id: Option<&str>,
        dir: Option<&str>,
    ) -> Result<Vec<(RoomMember, Option<String>, Option<String>)>, sqlx::Error> {
        let is_backward = dir.map(|d| d == "b").unwrap_or(false);
        let limit = limit.min(1000);

        // Build the base SELECT with user profile join
        let base = "SELECT rm.room_id, rm.user_id, rm.sender, rm.membership, rm.event_id, rm.event_type,
                   rm.display_name, rm.avatar_url, rm.is_banned, rm.invite_token, rm.updated_ts,
                   rm.joined_ts, rm.left_ts, rm.reason, rm.banned_by, rm.ban_reason, rm.banned_ts, rm.join_reason,
                   u.displayname as user_displayname, u.avatar_url as user_avatar_url
            FROM room_memberships rm
            LEFT JOIN users u ON rm.user_id = u.user_id";

        // Cursor operator: > for forward, < for backward
        let cursor_op = if is_backward { "<" } else { ">" };

        if let Some(nm) = not_membership {
            // With not_membership filter
            if let Some(from) = from_user_id {
                sqlx::query(&format!(
                    "{} WHERE rm.room_id = $1 AND rm.membership = $2 AND rm.membership != $3 AND rm.user_id {} $4 ORDER BY rm.user_id {} LIMIT $5",
                    base, cursor_op, if is_backward { "DESC" } else { "ASC" }
                ))
                .bind(room_id)
                .bind(membership_type)
                .bind(nm)
                .bind(from)
                .bind(limit)
                .fetch_all(&*self.pool)
                .await
            } else {
                sqlx::query(&format!(
                    "{} WHERE rm.room_id = $1 AND rm.membership = $2 AND rm.membership != $3 ORDER BY rm.user_id {} LIMIT $4",
                    base, if is_backward { "DESC" } else { "ASC" }
                ))
                .bind(room_id)
                .bind(membership_type)
                .bind(nm)
                .bind(limit)
                .fetch_all(&*self.pool)
                .await
            }
        } else if let Some(from) = from_user_id {
            sqlx::query(&format!(
                "{} WHERE rm.room_id = $1 AND rm.membership = $2 AND rm.user_id {} $3 ORDER BY rm.user_id {} LIMIT $4",
                base, cursor_op, if is_backward { "DESC" } else { "ASC" }
            ))
            .bind(room_id)
            .bind(membership_type)
            .bind(from)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        } else {
            sqlx::query(&format!(
                "{} WHERE rm.room_id = $1 AND rm.membership = $2 ORDER BY rm.user_id {} LIMIT $3",
                base, if is_backward { "DESC" } else { "ASC" }
            ))
            .bind(room_id)
            .bind(membership_type)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        }
        .map(|rows| {
            rows.iter()
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
                .collect()
        })
    }

    /// See [`get_members_batch`].
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
            r"
            SELECT room_id, user_id, sender, membership, event_id, event_type, display_name, avatar_url, is_banned, invite_token, updated_ts, joined_ts, left_ts, reason, banned_by, ban_reason, banned_ts, join_reason
            FROM room_memberships
            WHERE room_id = ANY($1) AND membership = $2
            ",
            room_ids,
            membership_type,
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

    /// See [`get_joined_members_batch`].
    pub async fn get_joined_members_batch(
        &self,
        room_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<RoomMember>>, sqlx::Error> {
        self.get_members_batch(room_ids, "join").await
    }

    /// See [`check_membership_batch`].
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
            r"
            SELECT user_id FROM room_memberships
            WHERE room_id = $1 AND user_id = ANY($2) AND membership = $3
            ",
            room_id,
            user_ids,
            membership_type,
        )
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
        let exists = sqlx::query_scalar!(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM room_memberships m1
                JOIN room_memberships m2 ON m1.room_id = m2.room_id
                WHERE m1.user_id = $1
                  AND m1.membership = 'join'
                  AND m2.membership = 'join'
                  AND m2.user_id LIKE $2
            ) AS "exists!"
            "#,
            user_id,
            &domain_pattern,
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(exists)
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
        let rows = sqlx::query_scalar!(
            r"
            SELECT DISTINCT m1.user_id
            FROM room_memberships m1
            JOIN room_memberships m2 ON m1.room_id = m2.room_id
            WHERE m1.user_id = ANY($1)
              AND m1.membership = 'join'
              AND m2.membership = 'join'
              AND m2.user_id LIKE $2
            ",
            user_ids,
            &domain_pattern,
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().collect())
    }

    /// See [`set_ban_reason`].
    ///
    /// `None` clears the column; the bound parameter is nullable, so this is
    /// the same statement either way.
    pub async fn set_ban_reason(&self, room_id: &str, user_id: &str, reason: Option<&str>) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            UPDATE room_memberships
            SET ban_reason = $3
            WHERE room_id = $1 AND user_id = $2
            ",
            room_id,
            user_id,
            reason,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`force_leave_membership`].
    pub async fn force_leave_membership(&self, room_id: &str, user_id: &str, now: i64) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            UPDATE room_memberships
            SET membership = 'leave',
                left_ts = $3,
                updated_ts = $3
            WHERE room_id = $1 AND user_id = $2
            ",
            room_id,
            user_id,
            now,
        )
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
        let rows = sqlx::query_scalar!(
            r#"
            SELECT DISTINCT
                SUBSTRING(user_id FROM POSITION(':' IN user_id) + 1) AS "server_name!"
            FROM room_memberships
            WHERE room_id = $1
              AND membership = 'join'
              AND POSITION(':' IN user_id) > 0
            "#,
            room_id,
        )
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows.into_iter().filter(|s| s != local_server_name).collect())
    }

    // ── MSC2666: Mutual Rooms (Get rooms in common with another user) ───

    /// Returns rooms where both users have `join` membership.
    /// Uses keyset pagination for O(n) intersection of sorted room lists.
    ///
    /// Returns `(rooms, next_batch_token)` where `next_batch_token` is the
    /// last room_id in the batch (for pagination via `after_room_id`).
    /// If `after_room_id` is None, starts from the beginning.
    pub async fn get_mutual_rooms_between(
        &self,
        user_id: &str,
        other_user_id: &str,
        limit: i64,
        after_room_id: Option<&str>,
    ) -> Result<(Vec<String>, Option<String>), sqlx::Error> {
        // Self-join to find rooms where BOTH users are members with 'join' status
        let rows = sqlx::query_scalar!(
            r#"
            SELECT a.room_id
              FROM room_memberships AS a
              JOIN room_memberships AS b ON a.room_id = b.room_id
             WHERE a.user_id = $1
               AND a.membership = 'join'
               AND b.user_id = $2
               AND b.membership = 'join'
               AND ($3::text IS NULL OR a.room_id > $3::text)
          ORDER BY a.room_id
             LIMIT $4
            "#,
            user_id,
            other_user_id,
            after_room_id,
            limit + 1, // fetch one extra to detect has_more
        )
        .fetch_all(&*self.pool)
        .await?;

        let has_more = rows.len() as i64 > limit;
        let rooms: Vec<String> = if has_more { rows.into_iter().take(limit as usize).collect() } else { rows };
        let next_batch_token = if has_more { rooms.last().cloned() } else { None };
        Ok((rooms, next_batch_token))
    }
}

#[cfg(test)]
mod tests;

// reason: `db_tests` 子模块声明上的属性：测试断言应当直接 panic，不是生产错误路径
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[cfg(test)]
mod db_tests;

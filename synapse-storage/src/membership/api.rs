use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use super::{RoomMember, UserRoomMembership};

/// Storage-agnostic API for room membership persistence.
///
/// Implemented by [`RoomMemberStorage`] (Postgres) and [`crate::test_mocks::InMemoryMemberStore`]
/// (in-memory). Services should accept `Arc<dyn MemberStoreApi>` so tests can
/// swap in the in-memory backend without a database.
///
/// # Mocking (STO-9 evaluation)
///
/// `mockall::automock` was evaluated and rejected for these traits:
/// - It requires mockall as a **regular** dependency (not dev-only) because
///   `#[automock]` generates code referencing `mockall` types during normal compilation.
/// - The hand-written fake pattern (`FakeAuth`, `InMemory*Store`) is more
///   maintainable: explicit, debuggable, and requires no extra dependencies.
/// - For complex traits, use `mockall::mock!` **in test modules** to generate
///   mocks without affecting production compilation.
///
/// Follows the same seam pattern as [`crate::event::EventReader`] / [`crate::event::EventWriter`].
#[async_trait]
pub trait MemberStoreApi: Send + Sync {
    /// Returns a reference to the database connection pool.
    fn pool(&self) -> &Arc<sqlx::PgPool>;

    /// See [`get_room_members`].
    async fn get_room_members(&self, room_id: &str, membership_type: &str) -> Result<Vec<RoomMember>, sqlx::Error>;

    /// See [`get_members_batch`].
    async fn get_members_batch(
        &self,
        room_ids: &[String],
        membership_type: &str,
    ) -> Result<HashMap<String, Vec<RoomMember>>, sqlx::Error>;

    /// See [`get_joined_rooms`].
    async fn get_joined_rooms(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error>;

    /// Cursor-paginated variant of [`Self::get_joined_rooms`].
    ///
    /// Returns at most `limit` rooms whose `room_id` is **strictly greater than**
    /// `after_room_id` (keyset pagination, no `OFFSET` penalty). An empty
    /// `after_room_id` starts from the beginning. Callers loop until fewer than
    /// `limit` rooms are returned to drain all joined rooms.
    ///
    /// Default impl falls back to the unbounded `get_joined_rooms`; the
    /// Postgres backend overrides this with `LIMIT $2 AND room_id > $3`.
    async fn get_joined_rooms_page(
        &self,
        user_id: &str,
        after_room_id: &str,
        limit: i64,
    ) -> Result<Vec<String>, sqlx::Error> {
        let _ = (after_room_id, limit);
        self.get_joined_rooms(user_id).await
    }

    /// See [`get_joined_room_count`].
    async fn get_joined_room_count(&self, user_id: &str) -> Result<i64, sqlx::Error>;

    /// See [`get_shared_room_users`].
    async fn get_shared_room_users(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error>;

    /// See [`get_sync_rooms`].
    async fn get_sync_rooms(&self, user_id: &str, include_leave: bool) -> Result<Vec<UserRoomMembership>, sqlx::Error>;

    /// Remove (leave) a member from a room. When `tx` is supplied the
    /// mutation is part of a caller-managed transaction (used by MSC4267
    /// leave+forget to atomically mark the membership as 'leave' and 'forget'
    /// in the same transaction).
    async fn remove_member(
        &self,
        room_id: &str,
        user_id: &str,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<(), sqlx::Error>;

    /// See [`is_member`].
    async fn is_member(&self, room_id: &str, user_id: &str) -> Result<bool, sqlx::Error>;

    /// See [`get_room_member`].
    async fn get_room_member(&self, room_id: &str, user_id: &str) -> Result<Option<RoomMember>, sqlx::Error>;

    /// Fetch members for a set of users in a single room, keyed by user id.
    ///
    /// Default impl falls back to per-user `get_room_member`; the Postgres
    /// backend overrides this with a single `ANY($2)` query to avoid N+1 on the
    /// device-list "left users" path.
    async fn get_room_members_by_user_ids(
        &self,
        room_id: &str,
        user_ids: &[String],
    ) -> Result<HashMap<String, RoomMember>, sqlx::Error> {
        let mut result = HashMap::new();
        for user_id in user_ids {
            if let Some(member) = self.get_room_member(room_id, user_id).await? {
                result.insert(user_id.clone(), member);
            }
        }
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    /// See [`add_member`].
    async fn add_member(
        &self,
        room_id: &str,
        user_id: &str,
        membership: &str,
        display_name: Option<&str>,
        join_reason: Option<&str>,
        sender: Option<&str>,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomMember, sqlx::Error>;

    // ── Extended membership queries (added for service-layer migration) ──

    /// See [`get_joined_members`].
    async fn get_joined_members(&self, room_id: &str) -> Result<Vec<RoomMember>, sqlx::Error>;

    /// See [`get_room_members_with_profiles`].
    async fn get_room_members_with_profiles(
        &self,
        room_id: &str,
        membership_type: &str,
    ) -> Result<Vec<(RoomMember, Option<String>, Option<String>)>, sqlx::Error>;

    /// See [`get_membership_history`].
    async fn get_membership_history(&self, room_id: &str, limit: i64) -> Result<Vec<RoomMember>, sqlx::Error>;

    /// See [`get_membership_state`].
    async fn get_membership_state(&self, room_id: &str, user_id: &str) -> Result<Option<String>, sqlx::Error>;

    /// See [`get_room_members_paginated`].
    async fn get_room_members_paginated(
        &self,
        room_id: &str,
        membership_type: &str,
        limit: i64,
        from_user_id: Option<&str>,
    ) -> Result<Vec<RoomMember>, sqlx::Error>;

    /// MSC4502: See [`get_room_members_paginated_with_profiles`].
    async fn get_room_members_paginated_with_profiles(
        &self,
        room_id: &str,
        membership_type: &str,
        not_membership: Option<&str>,
        limit: i64,
        from_user_id: Option<&str>,
        dir: Option<&str>,
    ) -> Result<Vec<(RoomMember, Option<String>, Option<String>)>, sqlx::Error>;

    /// See [`get_room_member_count`].
    async fn get_room_member_count(&self, room_id: &str) -> Result<i64, sqlx::Error>;

    /// See [`share_common_room`].
    async fn share_common_room(&self, user_id_1: &str, user_id_2: &str) -> Result<bool, sqlx::Error>;

    /// See [`share_common_rooms_batch`].
    async fn share_common_rooms_batch(
        &self,
        user_id: &str,
        other_user_ids: &[String],
    ) -> Result<Vec<String>, sqlx::Error>;

    /// See [`has_any_non_banned_member_from_server`].
    async fn has_any_non_banned_member_from_server(
        &self,
        room_id: &str,
        server_name: &str,
    ) -> Result<bool, sqlx::Error>;

    /// See [`user_shares_room_with_server`].
    async fn user_shares_room_with_server(&self, user_id: &str, server_name: &str) -> Result<bool, sqlx::Error>;

    /// See [`filter_users_sharing_room_with_server`].
    async fn filter_users_sharing_room_with_server(
        &self,
        user_ids: &[String],
        server_name: &str,
    ) -> Result<std::collections::HashSet<String>, sqlx::Error>;

    /// See [`ban_member`].
    async fn ban_member(&self, room_id: &str, user_id: &str, banned_by: &str) -> Result<(), sqlx::Error>;

    /// See [`unban_member`].
    async fn unban_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error>;

    /// See [`set_ban_reason`].
    async fn set_ban_reason(&self, room_id: &str, user_id: &str, reason: &str) -> Result<(), sqlx::Error>;

    /// See [`force_leave_membership`].
    async fn force_leave_membership(&self, room_id: &str, user_id: &str, now: i64) -> Result<(), sqlx::Error>;

    // ── Additional membership queries (added for state-service migration) ──

    /// Mark a room membership as 'forget' so the user no longer sees it in
    /// their room list. When `tx` is supplied the mutation is part of a
    /// caller-managed transaction (used by MSC4267 leave+forget to atomically
    /// apply leave + forget in a single DB transaction).
    async fn forget_member(
        &self,
        room_id: &str,
        user_id: &str,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<(), sqlx::Error>;

    /// See [`remove_all_members`].
    async fn remove_all_members(&self, room_id: &str) -> Result<(), sqlx::Error>;

    /// See [`get_joined_servers_in_room`].
    async fn get_joined_servers_in_room(
        &self,
        room_id: &str,
        local_server_name: &str,
    ) -> Result<Vec<String>, sqlx::Error>;

    // ── MSC2666: Mutual Rooms ──────────────────────────────────────

    /// See [`get_mutual_rooms_between`].
    async fn get_mutual_rooms_between(
        &self,
        user_id: &str,
        other_user_id: &str,
        limit: i64,
        after_room_id: Option<&str>,
    ) -> Result<(Vec<String>, Option<String>), sqlx::Error>;
}

// ── Delegation impl for the Postgres RoomMemberStorage ──────────────

#[async_trait]
impl MemberStoreApi for super::RoomMemberStorage {
    fn pool(&self) -> &Arc<sqlx::PgPool> {
        &self.pool
    }

    async fn get_room_members(&self, room_id: &str, membership_type: &str) -> Result<Vec<RoomMember>, sqlx::Error> {
        self.get_room_members(room_id, membership_type).await
    }

    async fn get_members_batch(
        &self,
        room_ids: &[String],
        membership_type: &str,
    ) -> Result<HashMap<String, Vec<RoomMember>>, sqlx::Error> {
        self.get_members_batch(room_ids, membership_type).await
    }

    async fn get_joined_rooms(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        self.get_joined_rooms(user_id).await
    }

    async fn get_joined_rooms_page(
        &self,
        user_id: &str,
        after_room_id: &str,
        limit: i64,
    ) -> Result<Vec<String>, sqlx::Error> {
        self.get_joined_rooms_page(user_id, after_room_id, limit).await
    }

    async fn get_joined_room_count(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        self.get_joined_room_count(user_id).await
    }

    async fn get_shared_room_users(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        self.get_shared_room_users(user_id).await
    }

    async fn get_sync_rooms(&self, user_id: &str, include_leave: bool) -> Result<Vec<UserRoomMembership>, sqlx::Error> {
        self.get_sync_rooms(user_id, include_leave).await
    }

    async fn remove_member(
        &self,
        room_id: &str,
        user_id: &str,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<(), sqlx::Error> {
        self.remove_member(room_id, user_id, tx).await
    }

    async fn is_member(&self, room_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        self.is_member(room_id, user_id).await
    }

    async fn get_room_member(&self, room_id: &str, user_id: &str) -> Result<Option<RoomMember>, sqlx::Error> {
        self.get_room_member(room_id, user_id).await
    }

    async fn get_room_members_by_user_ids(
        &self,
        room_id: &str,
        user_ids: &[String],
    ) -> Result<HashMap<String, RoomMember>, sqlx::Error> {
        self.get_room_members_by_user_ids(room_id, user_ids).await
    }

    async fn add_member(
        &self,
        room_id: &str,
        user_id: &str,
        membership: &str,
        display_name: Option<&str>,
        join_reason: Option<&str>,
        sender: Option<&str>,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomMember, sqlx::Error> {
        self.add_member(room_id, user_id, membership, display_name, join_reason, sender, tx).await
    }

    // ── Extended membership queries (delegated to inherent methods) ──

    async fn get_joined_members(&self, room_id: &str) -> Result<Vec<RoomMember>, sqlx::Error> {
        self.get_joined_members(room_id).await
    }

    async fn get_room_members_with_profiles(
        &self,
        room_id: &str,
        membership_type: &str,
    ) -> Result<Vec<(RoomMember, Option<String>, Option<String>)>, sqlx::Error> {
        self.get_room_members_with_profiles(room_id, membership_type).await
    }

    async fn get_membership_history(&self, room_id: &str, limit: i64) -> Result<Vec<RoomMember>, sqlx::Error> {
        self.get_membership_history(room_id, limit).await
    }

    async fn get_membership_state(&self, room_id: &str, user_id: &str) -> Result<Option<String>, sqlx::Error> {
        self.get_membership_state(room_id, user_id).await
    }

    async fn get_room_members_paginated(
        &self,
        room_id: &str,
        membership_type: &str,
        limit: i64,
        from_user_id: Option<&str>,
    ) -> Result<Vec<RoomMember>, sqlx::Error> {
        self.get_room_members_paginated(room_id, membership_type, limit, from_user_id).await
    }

    async fn get_room_members_paginated_with_profiles(
        &self,
        room_id: &str,
        membership_type: &str,
        not_membership: Option<&str>,
        limit: i64,
        from_user_id: Option<&str>,
        dir: Option<&str>,
    ) -> Result<Vec<(RoomMember, Option<String>, Option<String>)>, sqlx::Error> {
        self.get_room_members_paginated_with_profiles(
            room_id,
            membership_type,
            not_membership,
            limit,
            from_user_id,
            dir,
        )
        .await
    }

    async fn get_room_member_count(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        self.get_room_member_count(room_id).await
    }

    async fn share_common_room(&self, user_id_1: &str, user_id_2: &str) -> Result<bool, sqlx::Error> {
        self.share_common_room(user_id_1, user_id_2).await
    }

    async fn share_common_rooms_batch(
        &self,
        user_id: &str,
        other_user_ids: &[String],
    ) -> Result<Vec<String>, sqlx::Error> {
        self.share_common_rooms_batch(user_id, other_user_ids).await
    }

    async fn has_any_non_banned_member_from_server(
        &self,
        room_id: &str,
        server_name: &str,
    ) -> Result<bool, sqlx::Error> {
        self.has_any_non_banned_member_from_server(room_id, server_name).await
    }

    async fn user_shares_room_with_server(&self, user_id: &str, server_name: &str) -> Result<bool, sqlx::Error> {
        self.user_shares_room_with_server(user_id, server_name).await
    }

    async fn filter_users_sharing_room_with_server(
        &self,
        user_ids: &[String],
        server_name: &str,
    ) -> Result<std::collections::HashSet<String>, sqlx::Error> {
        self.filter_users_sharing_room_with_server(user_ids, server_name).await
    }

    async fn ban_member(&self, room_id: &str, user_id: &str, banned_by: &str) -> Result<(), sqlx::Error> {
        self.ban_member(room_id, user_id, banned_by).await
    }

    async fn unban_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        self.unban_member(room_id, user_id).await
    }

    async fn set_ban_reason(&self, room_id: &str, user_id: &str, reason: &str) -> Result<(), sqlx::Error> {
        self.set_ban_reason(room_id, user_id, reason).await
    }

    async fn force_leave_membership(&self, room_id: &str, user_id: &str, now: i64) -> Result<(), sqlx::Error> {
        self.force_leave_membership(room_id, user_id, now).await
    }

    async fn forget_member(
        &self,
        room_id: &str,
        user_id: &str,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<(), sqlx::Error> {
        self.forget_member(room_id, user_id, tx).await
    }

    async fn remove_all_members(&self, room_id: &str) -> Result<(), sqlx::Error> {
        self.remove_all_members(room_id).await
    }

    async fn get_joined_servers_in_room(
        &self,
        room_id: &str,
        local_server_name: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        self.get_joined_servers_in_room(room_id, local_server_name).await
    }

    async fn get_mutual_rooms_between(
        &self,
        user_id: &str,
        other_user_id: &str,
        limit: i64,
        after_room_id: Option<&str>,
    ) -> Result<(Vec<String>, Option<String>), sqlx::Error> {
        self.get_mutual_rooms_between(user_id, other_user_id, limit, after_room_id).await
    }
}

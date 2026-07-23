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

    async fn get_room_members(&self, room_id: &str, membership_type: &str) -> Result<Vec<RoomMember>, sqlx::Error>;

    async fn get_members_batch(
        &self,
        room_ids: &[String],
        membership_type: &str,
    ) -> Result<HashMap<String, Vec<RoomMember>>, sqlx::Error>;

    async fn get_joined_rooms(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error>;

    async fn get_joined_room_count(&self, user_id: &str) -> Result<i64, sqlx::Error>;

    async fn get_shared_room_users(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error>;

    async fn get_sync_rooms(&self, user_id: &str, include_leave: bool) -> Result<Vec<UserRoomMembership>, sqlx::Error>;

    async fn remove_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error>;

    async fn is_member(&self, room_id: &str, user_id: &str) -> Result<bool, sqlx::Error>;

    async fn get_room_member(&self, room_id: &str, user_id: &str) -> Result<Option<RoomMember>, sqlx::Error>;

    #[allow(clippy::too_many_arguments)]
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

    async fn get_joined_members(&self, room_id: &str) -> Result<Vec<RoomMember>, sqlx::Error>;

    async fn get_room_members_with_profiles(
        &self,
        room_id: &str,
        membership_type: &str,
    ) -> Result<Vec<(RoomMember, Option<String>, Option<String>)>, sqlx::Error>;

    async fn get_membership_history(&self, room_id: &str, limit: i64) -> Result<Vec<RoomMember>, sqlx::Error>;

    async fn get_membership_state(&self, room_id: &str, user_id: &str) -> Result<Option<String>, sqlx::Error>;

    async fn get_room_members_paginated(
        &self,
        room_id: &str,
        membership_type: &str,
        limit: i64,
        from_user_id: Option<&str>,
    ) -> Result<Vec<RoomMember>, sqlx::Error>;

    async fn get_room_member_count(&self, room_id: &str) -> Result<i64, sqlx::Error>;

    async fn share_common_room(&self, user_id_1: &str, user_id_2: &str) -> Result<bool, sqlx::Error>;

    async fn share_common_rooms_batch(
        &self,
        user_id: &str,
        other_user_ids: &[String],
    ) -> Result<Vec<String>, sqlx::Error>;

    async fn has_any_non_banned_member_from_server(
        &self,
        room_id: &str,
        server_name: &str,
    ) -> Result<bool, sqlx::Error>;

    async fn user_shares_room_with_server(&self, user_id: &str, server_name: &str) -> Result<bool, sqlx::Error>;

    async fn filter_users_sharing_room_with_server(
        &self,
        user_ids: &[String],
        server_name: &str,
    ) -> Result<std::collections::HashSet<String>, sqlx::Error>;

    async fn ban_member(&self, room_id: &str, user_id: &str, banned_by: &str) -> Result<(), sqlx::Error>;

    async fn unban_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error>;

    async fn set_ban_reason(&self, room_id: &str, user_id: &str, reason: &str) -> Result<(), sqlx::Error>;

    async fn force_leave_membership(&self, room_id: &str, user_id: &str, now: i64) -> Result<(), sqlx::Error>;

    // ── Additional membership queries (added for state-service migration) ──

    async fn forget_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error>;

    async fn remove_all_members(&self, room_id: &str) -> Result<(), sqlx::Error>;

    async fn get_joined_servers_in_room(
        &self,
        room_id: &str,
        local_server_name: &str,
    ) -> Result<Vec<String>, sqlx::Error>;
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

    async fn get_joined_room_count(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        self.get_joined_room_count(user_id).await
    }

    async fn get_shared_room_users(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        self.get_shared_room_users(user_id).await
    }

    async fn get_sync_rooms(&self, user_id: &str, include_leave: bool) -> Result<Vec<UserRoomMembership>, sqlx::Error> {
        self.get_sync_rooms(user_id, include_leave).await
    }

    async fn remove_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        self.remove_member(room_id, user_id).await
    }

    async fn is_member(&self, room_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        self.is_member(room_id, user_id).await
    }

    async fn get_room_member(&self, room_id: &str, user_id: &str) -> Result<Option<RoomMember>, sqlx::Error> {
        self.get_room_member(room_id, user_id).await
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

    async fn forget_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        self.forget_member(room_id, user_id).await
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
}

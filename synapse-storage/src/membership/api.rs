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
/// Follows the same seam pattern as [`crate::event::api::EventStoreApi`].
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
}

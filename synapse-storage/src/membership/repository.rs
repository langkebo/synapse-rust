use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::RoomMember;
use super::UserRoomMembership;

/// Repository trait for room-membership persistence operations.
#[async_trait]
#[allow(clippy::too_many_arguments)]
pub trait RoomMemberRepository: Send + Sync {
    /// Returns a reference to the database connection pool.
    fn pool(&self) -> &Arc<sqlx::PgPool>;

    /// Add or update a member in a room.
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

    /// Look up a single member by room and user.
    async fn get_member(&self, room_id: &str, user_id: &str) -> Result<Option<RoomMember>, sqlx::Error>;

    /// Fetch all members of a given membership type in a room.
    async fn get_room_members(&self, room_id: &str, membership_type: &str) -> Result<Vec<RoomMember>, sqlx::Error>;

    /// Check whether any user from the given server domain has a non-banned
    /// membership (join, invite, or leave) in the room.
    async fn has_any_non_banned_member_from_server(
        &self,
        room_id: &str,
        server_name: &str,
    ) -> Result<bool, sqlx::Error>;

    /// Count joined members in a room.
    async fn get_room_member_count(&self, room_id: &str) -> Result<i64, sqlx::Error>;

    /// Fetch paginated members by membership type.
    async fn get_room_members_paginated(
        &self,
        room_id: &str,
        membership_type: &str,
        limit: i64,
        from_user_id: Option<&str>,
    ) -> Result<Vec<RoomMember>, sqlx::Error>;

    /// Set a member's membership to 'leave'.
    async fn remove_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error>;

    /// Forget a room membership (sets membership to 'forget').
    async fn forget_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error>;

    /// Check whether a user has forgotten a room.
    async fn is_forgotten(&self, room_id: &str, user_id: &str) -> Result<bool, sqlx::Error>;

    /// Get users who share at least one joined room with the given user.
    async fn get_shared_room_users(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error>;

    /// Remove all members from a room.
    async fn remove_all_members(&self, room_id: &str) -> Result<(), sqlx::Error>;

    /// Ban a member from a room.
    async fn ban_member(&self, room_id: &str, user_id: &str, banned_by: &str) -> Result<(), sqlx::Error>;

    /// Unban a member from a room.
    async fn unban_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error>;

    /// Get the list of room IDs a user has joined.
    async fn get_joined_rooms(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error>;

    /// Get rooms for sync, optionally including 'leave' memberships.
    async fn get_sync_rooms(&self, user_id: &str, include_leave: bool) -> Result<Vec<UserRoomMembership>, sqlx::Error>;

    /// Get the membership state of a user in a room.
    async fn get_membership_state(&self, room_id: &str, user_id: &str) -> Result<Option<String>, sqlx::Error>;

    /// Count the number of rooms a user has joined.
    async fn get_joined_room_count(&self, user_id: &str) -> Result<i64, sqlx::Error>;

    /// Check whether a user is a joined member of a room.
    async fn is_member(&self, room_id: &str, user_id: &str) -> Result<bool, sqlx::Error>;

    /// Get a room member record (any membership state).
    async fn get_room_member(&self, room_id: &str, user_id: &str) -> Result<Option<RoomMember>, sqlx::Error>;

    /// Get all currently joined members of a room.
    async fn get_joined_members(&self, room_id: &str) -> Result<Vec<RoomMember>, sqlx::Error>;

    /// Get a single joined member by room and user.
    async fn get_joined_member(&self, room_id: &str, user_id: &str) -> Result<Option<RoomMember>, sqlx::Error>;

    /// Check whether two users share a common joined room.
    async fn share_common_room(&self, user_id_1: &str, user_id_2: &str) -> Result<bool, sqlx::Error>;

    /// Batch check which of `other_user_ids` share a joined room with `user_id`.
    async fn share_common_rooms_batch(
        &self,
        user_id: &str,
        other_user_ids: &[String],
    ) -> Result<Vec<String>, sqlx::Error>;

    /// Get membership history for a room, ordered by updated_ts desc.
    async fn get_membership_history(&self, room_id: &str, limit: i64) -> Result<Vec<RoomMember>, sqlx::Error>;

    /// Get joined rooms with their name, topic, and avatar_url.
    async fn get_joined_rooms_with_details(
        &self,
        user_id: &str,
    ) -> Result<Vec<(String, String, Option<String>, Option<String>)>, sqlx::Error>;

    /// Get room members with their user profile (displayname, avatar_url).
    async fn get_room_members_with_profiles(
        &self,
        room_id: &str,
        membership_type: &str,
    ) -> Result<Vec<(RoomMember, Option<String>, Option<String>)>, sqlx::Error>;

    /// Batch-load members by membership type across multiple rooms.
    async fn get_members_batch(
        &self,
        room_ids: &[String],
        membership_type: &str,
    ) -> Result<HashMap<String, Vec<RoomMember>>, sqlx::Error>;

    /// Convenience: `get_members_batch` filtered to 'join'.
    async fn get_joined_members_batch(
        &self,
        room_ids: &[String],
    ) -> Result<HashMap<String, Vec<RoomMember>>, sqlx::Error>;

    /// Check which of `user_ids` have the given membership type in a room.
    async fn check_membership_batch(
        &self,
        room_id: &str,
        user_ids: &[String],
        membership_type: &str,
    ) -> Result<HashSet<String>, sqlx::Error>;

    /// Check whether the given user shares a joined room with any member from
    /// the given server domain.
    async fn user_shares_room_with_server(&self, user_id: &str, server_name: &str) -> Result<bool, sqlx::Error>;

    /// Batch version of `user_shares_room_with_server`.
    async fn filter_users_sharing_room_with_server(
        &self,
        user_ids: &[String],
        server_name: &str,
    ) -> Result<HashSet<String>, sqlx::Error>;

    /// Set the ban reason for a banned member.
    async fn set_ban_reason(&self, room_id: &str, user_id: &str, reason: &str) -> Result<(), sqlx::Error>;

    /// Forcefully set a membership to 'leave' at a specific timestamp.
    async fn force_leave_membership(&self, room_id: &str, user_id: &str, now: i64) -> Result<(), sqlx::Error>;

    /// Get distinct server domains of all currently-joined members in a room,
    /// excluding the local server name.
    async fn get_joined_servers_in_room(
        &self,
        room_id: &str,
        local_server_name: &str,
    ) -> Result<Vec<String>, sqlx::Error>;
}

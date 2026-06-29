use async_trait::async_trait;

use super::models::Room;

/// Repository trait for room-level persistence operations.
///
/// The signatures have been verified against the inherent methods on
/// `RoomStorage`.  See the task-3 report for the full verification table.
///
/// CONCERN: `create_room` was changed from the original plan —
/// the plan used `room_type: Option<&str>` and `room_version: Option<&str>`,
/// but the concrete `RoomStorage::create_room` takes `join_rule: &str` and
/// `version: &str` (both required).  The trait now matches the concrete
/// signature exactly.
///
/// CONCERN: `set_room_public` does not exist as a concrete method;
/// the delegation calls `RoomStorage::set_room_directory` under the hood.
/// The trait method name was preserved because it better expresses intent.
///
/// CONCERN: `get_user_rooms` was changed from `Vec<Room>` to `Vec<String>`
/// because the inherent method only returns room IDs.
///
/// CONCERN: `get_public_rooms` was changed to drop the `since` parameter
/// because `RoomStorage::get_public_rooms` does not support it.
/// The paginated variant lives in `admin.rs` as
/// `get_public_rooms_paginated(limit, since_ts, since_room_id)`.
///
/// CONCERN: `search_room_directory` did not exist as a method.  A new
/// inherent method of the same name was added to `RoomStorage` in models.rs.
#[async_trait]
pub trait RoomRepository: Send + Sync {
    /// Look up a single room by ID.
    async fn get_room(&self, room_id: &str) -> Result<Option<Room>, sqlx::Error>;

    /// Batch-load multiple rooms by their IDs.
    async fn get_rooms_batch(&self, room_ids: &[String]) -> Result<Vec<Room>, sqlx::Error>;

    /// Insert a new room row.
    async fn create_room(
        &self,
        room_id: &str,
        creator: &str,
        join_rule: &str,
        room_version: &str,
        is_public: bool,
    ) -> Result<Room, sqlx::Error>;

    /// Update the room's display name.
    async fn update_room_name(&self, room_id: &str, name: &str) -> Result<(), sqlx::Error>;

    /// Update the room's topic.
    async fn update_room_topic(&self, room_id: &str, topic: &str) -> Result<(), sqlx::Error>;

    /// Set the public / private visibility of a room (updates both the
    /// `rooms` table and the `room_directory` table).
    async fn set_room_public(&self, room_id: &str, is_public: bool) -> Result<(), sqlx::Error>;

    /// Permanently remove a room from the database.
    async fn delete_room(&self, room_id: &str) -> Result<(), sqlx::Error>;

    /// Fetch public (world-readable) rooms up to `limit`.
    async fn get_public_rooms(&self, limit: i64) -> Result<Vec<Room>, sqlx::Error>;

    /// Return the list of room IDs a given user has joined.
    async fn get_user_rooms(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error>;

    /// Search the room directory (public rooms) by name / topic.
    async fn search_room_directory(
        &self,
        search_term: &str,
        limit: i64,
    ) -> Result<Vec<Room>, sqlx::Error>;
}

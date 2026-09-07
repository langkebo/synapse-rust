use async_trait::async_trait;
use std::sync::Arc;

use super::models::*;

/// Storage-agnostic API for room persistence.
///
/// Implemented by [`RoomStorage`] (Postgres) and [`crate::test_mocks::InMemoryRoomStore`]
/// (in-memory). Services should accept `Arc<dyn RoomStoreApi>` so tests can
/// swap in the in-memory backend without a database.
///
/// Follows the same seam pattern as [`crate::event::EventReader`] / [`crate::event::EventWriter`].
#[async_trait]
pub trait RoomStoreApi: Send + Sync {
    /// Returns a reference to the database connection pool.
    fn pool(&self) -> &Arc<sqlx::PgPool>;

    /// See [`create_room`].
    async fn create_room(
        &self,
        room_id: &str,
        creator: &str,
        join_rule: &str,
        version: &str,
        is_public: bool,
    ) -> Result<Room, sqlx::Error>;

    /// See [`create_room_in_tx`].
    async fn create_room_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        room_id: &str,
        creator: &str,
        join_rule: &str,
        version: &str,
        is_public: bool,
    ) -> Result<Room, sqlx::Error>;

    /// See [`get_room`].
    async fn get_room(&self, room_id: &str) -> Result<Option<Room>, sqlx::Error>;

    /// See [`room_exists`].
    async fn room_exists(&self, room_id: &str) -> Result<bool, sqlx::Error>;

    /// See [`get_public_rooms`].
    async fn get_public_rooms(&self, limit: i64) -> Result<Vec<Room>, sqlx::Error>;

    /// See [`get_room_count`].
    async fn get_room_count(&self) -> Result<i64, sqlx::Error>;

    /// See [`set_canonical_alias`].
    async fn set_canonical_alias(&self, room_id: &str, alias: Option<&str>) -> Result<(), sqlx::Error>;

    /// See [`set_room_alias`].
    async fn set_room_alias(&self, room_id: &str, alias: &str, created_by: &str) -> Result<(), sqlx::Error>;

    /// See [`update_join_rule_in_tx`].
    async fn update_join_rule_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        room_id: &str,
        join_rule: &str,
    ) -> Result<(), sqlx::Error>;

    /// See [`decrement_member_count`].
    async fn decrement_member_count(
        &self,
        room_id: &str,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<(), sqlx::Error>;

    /// Batch counterpart of `decrement_member_count`. Touches the `updated_ts`
    /// of each affected `room_summaries` row in a single UPDATE.
    ///
    /// Note: actual member-count columns are maintained by a database trigger
    /// on `room_memberships` (see v11 schema), so this method only refreshes
    /// the summary's `updated_ts`. Use after batch `remove_member` calls to
    /// avoid N round-trips.
    async fn decrement_member_counts_batch(&self, room_ids: &[String]) -> Result<u64, sqlx::Error>;

    /// See [`update_room_name`].
    async fn update_room_name(&self, room_id: &str, name: &str) -> Result<(), sqlx::Error>;

    /// See [`update_room_name_in_tx`].
    async fn update_room_name_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        room_id: &str,
        name: &str,
    ) -> Result<(), sqlx::Error>;

    /// See [`update_room_topic`].
    async fn update_room_topic(&self, room_id: &str, topic: &str) -> Result<(), sqlx::Error>;

    /// See [`update_room_topic_in_tx`].
    async fn update_room_topic_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        room_id: &str,
        topic: &str,
    ) -> Result<(), sqlx::Error>;

    /// See [`get_room_aliases`].
    async fn get_room_aliases(&self, room_id: &str) -> Result<Vec<String>, sqlx::Error>;

    /// See [`get_room_by_alias`].
    async fn get_room_by_alias(&self, alias: &str) -> Result<Option<String>, sqlx::Error>;

    /// See [`remove_room_alias`].
    async fn remove_room_alias(&self, room_id: &str) -> Result<(), sqlx::Error>;

    /// See [`remove_room_alias_by_name`].
    async fn remove_room_alias_by_name(&self, alias: &str) -> Result<(), sqlx::Error>;

    /// See [`is_room_in_directory`].
    async fn is_room_in_directory(&self, room_id: &str) -> Result<bool, sqlx::Error>;

    /// See [`set_room_directory`].
    async fn set_room_directory(&self, room_id: &str, is_public: bool) -> Result<(), sqlx::Error>;

    /// See [`remove_room_directory`].
    async fn remove_room_directory(&self, room_id: &str) -> Result<(), sqlx::Error>;

    // ── receipts / read markers ──────────────────────────────────────────

    /// See [`add_receipt`].
    async fn add_receipt(
        &self,
        user_id: &str,
        sent_to: &str,
        room_id: &str,
        event_id: &str,
        receipt_type: &str,
        data: &serde_json::Value,
    ) -> Result<(), sqlx::Error>;

    /// See [`get_receipts`].
    async fn get_receipts(
        &self,
        room_id: &str,
        receipt_type: &str,
        event_id: &str,
    ) -> Result<Vec<Receipt>, sqlx::Error>;

    /// See [`update_read_marker_with_type`].
    async fn update_read_marker_with_type(
        &self,
        room_id: &str,
        user_id: &str,
        event_id: &str,
        marker_type: &str,
    ) -> Result<(), sqlx::Error>;

    /// MSC4446: Update read marker with monotonicity check.
    /// Returns `true` if updated, `false` if silently dropped (backward move).
    async fn update_read_marker_monotonic(
        &self,
        room_id: &str,
        user_id: &str,
        event_id: &str,
        marker_type: &str,
        allow_backward: bool,
    ) -> Result<bool, sqlx::Error>;

    // ── Extended room queries (added for service-layer migration) ──

    /// See [`get_rooms_batch`].
    async fn get_rooms_batch(&self, room_ids: &[String]) -> Result<Vec<Room>, sqlx::Error>;

    /// See [`increment_member_count`].
    async fn increment_member_count(&self, room_id: &str) -> Result<(), sqlx::Error>;

    /// See [`get_user_rooms_paginated`].
    async fn get_user_rooms_paginated(
        &self,
        user_id: &str,
        limit: i64,
        from_room_id: Option<&str>,
    ) -> Result<Vec<String>, sqlx::Error>;

    // ── Admin / directory / stats queries (added for state-service migration) ──

    /// See [`get_public_rooms_paginated`].
    async fn get_public_rooms_paginated(
        &self,
        limit: i64,
        since_ts: Option<i64>,
        since_room_id: Option<&str>,
    ) -> Result<Vec<Room>, sqlx::Error>;

    /// See [`count_public_rooms`].
    async fn count_public_rooms(&self) -> Result<i64, sqlx::Error>;

    /// See [`get_all_rooms_with_members`].
    async fn get_all_rooms_with_members(
        &self,
        limit: i64,
        from: Option<RoomSearchCursor>,
        order_by: RoomSearchOrder,
    ) -> Result<(Vec<(Room, i64)>, Option<String>), sqlx::Error>;

    /// See [`get_user_room_list_summary`].
    async fn get_user_room_list_summary(
        &self,
        user_id: &str,
    ) -> Result<Vec<(String, String, String, String)>, sqlx::Error>;

    /// See [`delete_room`].
    async fn delete_room(&self, room_id: &str) -> Result<(), sqlx::Error>;

    /// See [`shutdown_room`].
    async fn shutdown_room(&self, room_id: &str) -> Result<(), sqlx::Error>;

    /// See [`block_room`].
    async fn block_room(
        &self,
        room_id: &str,
        blocked_at: i64,
        blocked_by: &str,
        reason: Option<&str>,
    ) -> Result<(), sqlx::Error>;

    /// See [`get_room_block_status`].
    async fn get_room_block_status(&self, room_id: &str) -> Result<Option<i64>, sqlx::Error>;

    /// See [`unblock_room`].
    async fn unblock_room(&self, room_id: &str) -> Result<(), sqlx::Error>;

    /// See [`get_room_stats_overview`].
    async fn get_room_stats_overview(&self) -> Result<serde_json::Value, sqlx::Error>;

    /// See [`get_single_room_stats`].
    async fn get_single_room_stats(&self, room_id: &str) -> Result<Option<serde_json::Value>, sqlx::Error>;

    /// See [`get_room_listings_status`].
    async fn get_room_listings_status(&self, room_id: &str) -> Result<Option<(bool, bool)>, sqlx::Error>;

    /// See [`set_room_public_with_directory`].
    async fn set_room_public_with_directory(&self, room_id: &str) -> Result<bool, sqlx::Error>;

    /// See [`set_room_private_with_directory`].
    async fn set_room_private_with_directory(&self, room_id: &str) -> Result<bool, sqlx::Error>;

    /// See [`get_room_version_only`].
    async fn get_room_version_only(&self, room_id: &str) -> Result<Option<String>, sqlx::Error>;

    /// See [`search_all_rooms_admin`].
    async fn search_all_rooms_admin(
        &self,
        search_term: Option<&str>,
        limit: i64,
        order_by: RoomSearchOrder,
        cursor: Option<RoomSearchCursor>,
        is_public: Option<bool>,
        is_encrypted: Option<bool>,
    ) -> Result<(Vec<serde_json::Value>, i64, Option<String>), sqlx::Error>;

    /// See [`cleanup_abnormal_data`].
    async fn cleanup_abnormal_data(&self, min_age_ms: Option<i64>) -> Result<serde_json::Value, sqlx::Error>;
}

// ── Delegation impl for the Postgres RoomStorage ────────────────────

#[async_trait]
impl RoomStoreApi for super::RoomStorage {
    fn pool(&self) -> &Arc<sqlx::PgPool> {
        &self.pool
    }

    async fn create_room(
        &self,
        room_id: &str,
        creator: &str,
        join_rule: &str,
        version: &str,
        is_public: bool,
    ) -> Result<Room, sqlx::Error> {
        self.create_room(room_id, creator, join_rule, version, is_public).await
    }

    async fn create_room_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        room_id: &str,
        creator: &str,
        join_rule: &str,
        version: &str,
        is_public: bool,
    ) -> Result<Room, sqlx::Error> {
        self.create_room_in_tx(tx, room_id, creator, join_rule, version, is_public).await
    }

    async fn get_room(&self, room_id: &str) -> Result<Option<Room>, sqlx::Error> {
        self.get_room(room_id).await
    }

    async fn room_exists(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        self.room_exists(room_id).await
    }

    async fn get_public_rooms(&self, limit: i64) -> Result<Vec<Room>, sqlx::Error> {
        self.get_public_rooms(limit).await
    }

    async fn get_room_count(&self) -> Result<i64, sqlx::Error> {
        self.get_room_count().await
    }

    async fn set_canonical_alias(&self, room_id: &str, alias: Option<&str>) -> Result<(), sqlx::Error> {
        self.set_canonical_alias(room_id, alias).await
    }

    async fn set_room_alias(&self, room_id: &str, alias: &str, created_by: &str) -> Result<(), sqlx::Error> {
        self.set_room_alias(room_id, alias, created_by).await
    }

    async fn update_join_rule_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        room_id: &str,
        join_rule: &str,
    ) -> Result<(), sqlx::Error> {
        self.update_join_rule_in_tx(tx, room_id, join_rule).await
    }

    async fn decrement_member_count(
        &self,
        room_id: &str,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<(), sqlx::Error> {
        self.decrement_member_count(room_id, tx).await
    }

    async fn decrement_member_counts_batch(&self, room_ids: &[String]) -> Result<u64, sqlx::Error> {
        self.decrement_member_counts_batch(room_ids).await
    }

    async fn update_room_name(&self, room_id: &str, name: &str) -> Result<(), sqlx::Error> {
        self.update_room_name(room_id, name).await
    }

    async fn update_room_name_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        room_id: &str,
        name: &str,
    ) -> Result<(), sqlx::Error> {
        self.update_room_name_in_tx(tx, room_id, name).await
    }

    async fn update_room_topic(&self, room_id: &str, topic: &str) -> Result<(), sqlx::Error> {
        self.update_room_topic(room_id, topic).await
    }

    async fn update_room_topic_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        room_id: &str,
        topic: &str,
    ) -> Result<(), sqlx::Error> {
        self.update_room_topic_in_tx(tx, room_id, topic).await
    }

    async fn get_room_aliases(&self, room_id: &str) -> Result<Vec<String>, sqlx::Error> {
        self.get_room_aliases(room_id).await
    }

    async fn get_room_by_alias(&self, alias: &str) -> Result<Option<String>, sqlx::Error> {
        self.get_room_by_alias(alias).await
    }

    async fn remove_room_alias(&self, room_id: &str) -> Result<(), sqlx::Error> {
        self.remove_room_alias(room_id).await
    }

    async fn remove_room_alias_by_name(&self, alias: &str) -> Result<(), sqlx::Error> {
        self.remove_room_alias_by_name(alias).await
    }

    async fn is_room_in_directory(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        self.is_room_in_directory(room_id).await
    }

    async fn set_room_directory(&self, room_id: &str, is_public: bool) -> Result<(), sqlx::Error> {
        self.set_room_directory(room_id, is_public).await
    }

    async fn remove_room_directory(&self, room_id: &str) -> Result<(), sqlx::Error> {
        self.remove_room_directory(room_id).await
    }

    // ── receipts / read markers ──────────────────────────────────────────

    async fn add_receipt(
        &self,
        user_id: &str,
        sent_to: &str,
        room_id: &str,
        event_id: &str,
        receipt_type: &str,
        data: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        self.add_receipt(user_id, sent_to, room_id, event_id, receipt_type, data).await
    }

    async fn get_receipts(
        &self,
        room_id: &str,
        receipt_type: &str,
        event_id: &str,
    ) -> Result<Vec<Receipt>, sqlx::Error> {
        self.get_receipts(room_id, receipt_type, event_id).await
    }

    async fn update_read_marker_with_type(
        &self,
        room_id: &str,
        user_id: &str,
        event_id: &str,
        marker_type: &str,
    ) -> Result<(), sqlx::Error> {
        self.update_read_marker_with_type(room_id, user_id, event_id, marker_type).await
    }

    async fn update_read_marker_monotonic(
        &self,
        room_id: &str,
        user_id: &str,
        event_id: &str,
        marker_type: &str,
        allow_backward: bool,
    ) -> Result<bool, sqlx::Error> {
        self.update_read_marker_monotonic(room_id, user_id, event_id, marker_type, allow_backward).await
    }

    // ── Extended room queries (delegated to inherent methods) ──

    async fn get_rooms_batch(&self, room_ids: &[String]) -> Result<Vec<Room>, sqlx::Error> {
        self.get_rooms_batch(room_ids).await
    }

    async fn increment_member_count(&self, room_id: &str) -> Result<(), sqlx::Error> {
        self.increment_member_count(room_id).await
    }

    async fn get_user_rooms_paginated(
        &self,
        user_id: &str,
        limit: i64,
        from_room_id: Option<&str>,
    ) -> Result<Vec<String>, sqlx::Error> {
        self.get_user_rooms_paginated(user_id, limit, from_room_id).await
    }

    // ── Admin / directory / stats queries (delegated to inherent methods) ──

    async fn get_public_rooms_paginated(
        &self,
        limit: i64,
        since_ts: Option<i64>,
        since_room_id: Option<&str>,
    ) -> Result<Vec<Room>, sqlx::Error> {
        self.get_public_rooms_paginated(limit, since_ts, since_room_id).await
    }

    async fn count_public_rooms(&self) -> Result<i64, sqlx::Error> {
        self.count_public_rooms().await
    }

    async fn get_all_rooms_with_members(
        &self,
        limit: i64,
        from: Option<RoomSearchCursor>,
        order_by: RoomSearchOrder,
    ) -> Result<(Vec<(Room, i64)>, Option<String>), sqlx::Error> {
        self.get_all_rooms_with_members(limit, from, order_by).await
    }

    async fn get_user_room_list_summary(
        &self,
        user_id: &str,
    ) -> Result<Vec<(String, String, String, String)>, sqlx::Error> {
        self.get_user_room_list_summary(user_id).await
    }

    async fn delete_room(&self, room_id: &str) -> Result<(), sqlx::Error> {
        // Default impl ignores the `u64` (events deleted) returned by
        // RoomStorage::delete_room so that this trait stays simple.
        // Implementations that need the count can call the inherent method.
        let _ = self.delete_room(room_id).await?;
        Ok(())
    }

    async fn shutdown_room(&self, room_id: &str) -> Result<(), sqlx::Error> {
        self.shutdown_room(room_id).await
    }

    async fn block_room(
        &self,
        room_id: &str,
        blocked_at: i64,
        blocked_by: &str,
        reason: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        self.block_room(room_id, blocked_at, blocked_by, reason).await
    }

    async fn get_room_block_status(&self, room_id: &str) -> Result<Option<i64>, sqlx::Error> {
        self.get_room_block_status(room_id).await
    }

    async fn unblock_room(&self, room_id: &str) -> Result<(), sqlx::Error> {
        self.unblock_room(room_id).await
    }

    async fn get_room_stats_overview(&self) -> Result<serde_json::Value, sqlx::Error> {
        self.get_room_stats_overview().await
    }

    async fn get_single_room_stats(&self, room_id: &str) -> Result<Option<serde_json::Value>, sqlx::Error> {
        self.get_single_room_stats(room_id).await
    }

    async fn get_room_listings_status(&self, room_id: &str) -> Result<Option<(bool, bool)>, sqlx::Error> {
        self.get_room_listings_status(room_id).await
    }

    async fn set_room_public_with_directory(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        self.set_room_public_with_directory(room_id).await
    }

    async fn set_room_private_with_directory(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        self.set_room_private_with_directory(room_id).await
    }

    async fn get_room_version_only(&self, room_id: &str) -> Result<Option<String>, sqlx::Error> {
        self.get_room_version_only(room_id).await
    }

    async fn search_all_rooms_admin(
        &self,
        search_term: Option<&str>,
        limit: i64,
        order_by: RoomSearchOrder,
        cursor: Option<RoomSearchCursor>,
        is_public: Option<bool>,
        is_encrypted: Option<bool>,
    ) -> Result<(Vec<serde_json::Value>, i64, Option<String>), sqlx::Error> {
        self.search_all_rooms_admin(search_term, limit, order_by, cursor, is_public, is_encrypted).await
    }

    async fn cleanup_abnormal_data(&self, min_age_ms: Option<i64>) -> Result<serde_json::Value, sqlx::Error> {
        self.cleanup_abnormal_data(min_age_ms).await
    }
}

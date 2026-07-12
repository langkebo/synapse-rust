use async_trait::async_trait;
use std::sync::Arc;

use super::models::*;

/// Storage-agnostic API for room persistence.
///
/// Implemented by [`RoomStorage`] (Postgres) and [`crate::test_mocks::InMemoryRoomStore`]
/// (in-memory). Services should accept `Arc<dyn RoomStoreApi>` so tests can
/// swap in the in-memory backend without a database.
///
/// Follows the same seam pattern as [`crate::event::api::EventStoreApi`].
#[async_trait]
pub trait RoomStoreApi: Send + Sync {
    /// Returns a reference to the database connection pool.
    fn pool(&self) -> &Arc<sqlx::PgPool>;

    async fn create_room(
        &self,
        room_id: &str,
        creator: &str,
        join_rule: &str,
        version: &str,
        is_public: bool,
    ) -> Result<Room, sqlx::Error>;

    async fn create_room_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        room_id: &str,
        creator: &str,
        join_rule: &str,
        version: &str,
        is_public: bool,
    ) -> Result<Room, sqlx::Error>;

    async fn get_room(&self, room_id: &str) -> Result<Option<Room>, sqlx::Error>;

    async fn room_exists(&self, room_id: &str) -> Result<bool, sqlx::Error>;

    async fn get_public_rooms(&self, limit: i64) -> Result<Vec<Room>, sqlx::Error>;

    async fn get_room_count(&self) -> Result<i64, sqlx::Error>;

    async fn set_canonical_alias(&self, room_id: &str, alias: Option<&str>) -> Result<(), sqlx::Error>;

    async fn set_room_alias(&self, room_id: &str, alias: &str, created_by: &str) -> Result<(), sqlx::Error>;

    async fn update_join_rule_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        room_id: &str,
        join_rule: &str,
    ) -> Result<(), sqlx::Error>;

    async fn decrement_member_count(&self, room_id: &str) -> Result<(), sqlx::Error>;

    async fn update_room_name(&self, room_id: &str, name: &str) -> Result<(), sqlx::Error>;

    async fn update_room_name_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        room_id: &str,
        name: &str,
    ) -> Result<(), sqlx::Error>;

    async fn update_room_topic(&self, room_id: &str, topic: &str) -> Result<(), sqlx::Error>;

    async fn update_room_topic_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        room_id: &str,
        topic: &str,
    ) -> Result<(), sqlx::Error>;

    async fn get_room_aliases(&self, room_id: &str) -> Result<Vec<String>, sqlx::Error>;

    async fn get_room_by_alias(&self, alias: &str) -> Result<Option<String>, sqlx::Error>;

    async fn remove_room_alias(&self, room_id: &str) -> Result<(), sqlx::Error>;

    async fn remove_room_alias_by_name(&self, alias: &str) -> Result<(), sqlx::Error>;

    async fn is_room_in_directory(&self, room_id: &str) -> Result<bool, sqlx::Error>;

    async fn set_room_directory(&self, room_id: &str, is_public: bool) -> Result<(), sqlx::Error>;

    async fn remove_room_directory(&self, room_id: &str) -> Result<(), sqlx::Error>;

    // ── receipts / read markers ──────────────────────────────────────────

    async fn add_receipt(
        &self,
        user_id: &str,
        sent_to: &str,
        room_id: &str,
        event_id: &str,
        receipt_type: &str,
        data: &serde_json::Value,
    ) -> Result<(), sqlx::Error>;

    async fn get_receipts(
        &self,
        room_id: &str,
        receipt_type: &str,
        event_id: &str,
    ) -> Result<Vec<Receipt>, sqlx::Error>;

    async fn update_read_marker_with_type(
        &self,
        room_id: &str,
        user_id: &str,
        event_id: &str,
        marker_type: &str,
    ) -> Result<(), sqlx::Error>;

    // ── Extended room queries (added for service-layer migration) ──

    async fn get_rooms_batch(&self, room_ids: &[String]) -> Result<Vec<Room>, sqlx::Error>;

    async fn increment_member_count(&self, room_id: &str) -> Result<(), sqlx::Error>;

    async fn get_user_rooms_paginated(
        &self,
        user_id: &str,
        limit: i64,
        from_room_id: Option<&str>,
    ) -> Result<Vec<String>, sqlx::Error>;

    // ── Admin / directory / stats queries (added for state-service migration) ──

    async fn get_public_rooms_paginated(
        &self,
        limit: i64,
        since_ts: Option<i64>,
        since_room_id: Option<&str>,
    ) -> Result<Vec<Room>, sqlx::Error>;

    async fn count_public_rooms(&self) -> Result<i64, sqlx::Error>;

    async fn get_all_rooms_with_members(
        &self,
        limit: i64,
        from: Option<RoomSearchCursor>,
        order_by: RoomSearchOrder,
    ) -> Result<(Vec<(Room, i64)>, Option<String>), sqlx::Error>;

    async fn get_user_room_list_summary(
        &self,
        user_id: &str,
    ) -> Result<Vec<(String, String, String, String)>, sqlx::Error>;

    async fn delete_room(&self, room_id: &str) -> Result<(), sqlx::Error>;

    async fn shutdown_room(&self, room_id: &str) -> Result<(), sqlx::Error>;

    async fn block_room(
        &self,
        room_id: &str,
        blocked_at: i64,
        blocked_by: &str,
        reason: Option<&str>,
    ) -> Result<(), sqlx::Error>;

    async fn get_room_block_status(&self, room_id: &str) -> Result<Option<i64>, sqlx::Error>;

    async fn unblock_room(&self, room_id: &str) -> Result<(), sqlx::Error>;

    async fn get_room_stats_overview(&self) -> Result<serde_json::Value, sqlx::Error>;

    async fn get_single_room_stats(&self, room_id: &str) -> Result<Option<serde_json::Value>, sqlx::Error>;

    async fn get_room_listings_status(&self, room_id: &str) -> Result<Option<(bool, bool)>, sqlx::Error>;

    async fn set_room_public_with_directory(&self, room_id: &str) -> Result<bool, sqlx::Error>;

    async fn set_room_private_with_directory(&self, room_id: &str) -> Result<bool, sqlx::Error>;

    async fn get_room_version_only(&self, room_id: &str) -> Result<Option<String>, sqlx::Error>;

    async fn search_all_rooms_admin(
        &self,
        search_term: Option<&str>,
        limit: i64,
        order_by: RoomSearchOrder,
        cursor: Option<RoomSearchCursor>,
        is_public: Option<bool>,
        is_encrypted: Option<bool>,
    ) -> Result<(Vec<serde_json::Value>, i64, Option<String>), sqlx::Error>;

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

    async fn decrement_member_count(&self, room_id: &str) -> Result<(), sqlx::Error> {
        self.decrement_member_count(room_id).await
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
        self.delete_room(room_id).await
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

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

    async fn get_unread_counts(&self, room_id: &str, user_id: &str) -> Result<RoomUnreadCounts, sqlx::Error>;

    async fn get_unread_counts_batch(
        &self,
        room_ids: &[String],
        user_id: &str,
    ) -> Result<Vec<RoomUnreadCounts>, sqlx::Error>;

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

    async fn copy_room_state(&self, source_room_id: &str, target_room_id: &str) -> Result<(), sqlx::Error>;

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

    async fn get_unread_counts(&self, room_id: &str, user_id: &str) -> Result<RoomUnreadCounts, sqlx::Error> {
        self.get_unread_counts(room_id, user_id).await
    }

    async fn get_unread_counts_batch(
        &self,
        room_ids: &[String],
        user_id: &str,
    ) -> Result<Vec<RoomUnreadCounts>, sqlx::Error> {
        self.get_unread_counts_batch(room_ids, user_id).await
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

    async fn copy_room_state(&self, source_room_id: &str, target_room_id: &str) -> Result<(), sqlx::Error> {
        self.copy_room_state(source_room_id, target_room_id).await
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
}

use async_trait::async_trait;
use std::sync::Arc;

use super::models::Room;

/// Repository trait for room-level persistence operations.
#[async_trait]
pub trait RoomRepository: Send + Sync {
    /// Returns a reference to the database connection pool.
    fn pool(&self) -> &Arc<sqlx::PgPool>;

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

    /// Like `create_room` but within an existing transaction.
    async fn create_room_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        room_id: &str,
        creator: &str,
        join_rule: &str,
        version: &str,
        is_public: bool,
    ) -> Result<Room, sqlx::Error>;

    /// Update the room's display name.
    async fn update_room_name(&self, room_id: &str, name: &str) -> Result<(), sqlx::Error>;

    /// Update the room's display name within a transaction.
    async fn update_room_name_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        room_id: &str,
        name: &str,
    ) -> Result<(), sqlx::Error>;

    /// Update the room's topic.
    async fn update_room_topic(&self, room_id: &str, topic: &str) -> Result<(), sqlx::Error>;

    /// Update the room's topic within a transaction.
    async fn update_room_topic_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        room_id: &str,
        topic: &str,
    ) -> Result<(), sqlx::Error>;

    /// Update the join_rules in a transaction.
    async fn update_join_rule_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        room_id: &str,
        join_rule: &str,
    ) -> Result<(), sqlx::Error>;

    /// Set the public / private visibility of a room (updates both the
    /// `rooms` table and the `room_directory` table).
    async fn set_room_public(&self, room_id: &str, is_public: bool) -> Result<(), sqlx::Error>;

    /// Permanently remove a room from the database.
    async fn delete_room(&self, room_id: &str) -> Result<(), sqlx::Error>;

    /// Fetch public (world-readable) rooms up to `limit`.
    async fn get_public_rooms(&self, limit: i64) -> Result<Vec<Room>, sqlx::Error>;

    /// Fetch paginated public rooms.
    async fn get_public_rooms_paginated(
        &self,
        limit: i64,
        since_ts: Option<i64>,
        since_room_id: Option<&str>,
    ) -> Result<Vec<Room>, sqlx::Error>;

    /// Count total public rooms.
    async fn count_public_rooms(&self) -> Result<i64, sqlx::Error>;

    /// Return the list of room IDs a given user has joined.
    async fn get_user_rooms(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error>;

    /// Search the room directory (public rooms) by name / topic.
    async fn search_room_directory(&self, search_term: &str, limit: i64) -> Result<Vec<Room>, sqlx::Error>;

    // -- aliases --

    async fn get_room_aliases(&self, room_id: &str) -> Result<Vec<String>, sqlx::Error>;

    async fn set_room_alias(&self, room_id: &str, alias: &str, _created_by: &str) -> Result<(), sqlx::Error>;

    async fn get_room_by_alias(&self, alias: &str) -> Result<Option<String>, sqlx::Error>;

    async fn remove_room_alias(&self, room_id: &str) -> Result<(), sqlx::Error>;

    async fn remove_room_alias_by_name(&self, alias: &str) -> Result<(), sqlx::Error>;

    // -- directory --

    async fn set_room_directory(&self, room_id: &str, is_public: bool) -> Result<(), sqlx::Error>;

    async fn is_room_in_directory(&self, room_id: &str) -> Result<bool, sqlx::Error>;

    async fn remove_room_directory(&self, room_id: &str) -> Result<(), sqlx::Error>;

    // -- canonical alias --

    async fn set_canonical_alias(&self, room_id: &str, alias: Option<&str>) -> Result<(), sqlx::Error>;

    // -- membership helpers --

    async fn increment_member_count(&self, room_id: &str) -> Result<(), sqlx::Error>;

    async fn decrement_member_count(&self, room_id: &str) -> Result<(), sqlx::Error>;

    // -- receipts --

    async fn add_receipt(
        &self,
        user_id: &str,
        receipt_user_id: &str,
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
    ) -> Result<Vec<super::models::Receipt>, sqlx::Error>;

    // -- read markers --

    async fn update_read_marker_with_type(
        &self,
        room_id: &str,
        user_id: &str,
        event_id: &str,
        marker_type: &str,
    ) -> Result<(), sqlx::Error>;

    // -- room state copy --

    async fn copy_room_state(&self, source_room_id: &str, target_room_id: &str) -> Result<(), sqlx::Error>;

    // -- room existence --

    async fn room_exists(&self, room_id: &str) -> Result<bool, sqlx::Error>;

    // -- room count --

    async fn get_room_count(&self) -> Result<i64, sqlx::Error>;

    // -- room version --

    async fn get_room_version_only(&self, room_id: &str) -> Result<Option<String>, sqlx::Error>;

    // -- moderation --

    async fn block_room(
        &self,
        room_id: &str,
        blocked_at: i64,
        blocked_by: &str,
        reason: Option<&str>,
    ) -> Result<(), sqlx::Error>;

    async fn unblock_room(&self, room_id: &str) -> Result<(), sqlx::Error>;

    async fn get_room_block_status(&self, room_id: &str) -> Result<Option<i64>, sqlx::Error>;

    // -- shutdown --

    async fn shutdown_room(&self, room_id: &str) -> Result<(), sqlx::Error>;

    // -- stats --

    async fn get_room_stats_overview(&self) -> Result<serde_json::Value, sqlx::Error>;

    async fn get_single_room_stats(&self, room_id: &str) -> Result<Option<serde_json::Value>, sqlx::Error>;

    // -- listings --

    async fn get_room_listings_status(&self, room_id: &str) -> Result<Option<(bool, bool)>, sqlx::Error>;

    async fn set_room_public_with_directory(&self, room_id: &str) -> Result<bool, sqlx::Error>;

    async fn set_room_private_with_directory(&self, room_id: &str) -> Result<bool, sqlx::Error>;

    // -- user room list summary --

    async fn get_user_room_list_summary(
        &self,
        user_id: &str,
    ) -> Result<Vec<(String, String, String, String)>, sqlx::Error>;

    // -- all rooms with members --

    async fn get_all_rooms_with_members(
        &self,
        limit: i64,
        from: Option<super::models::RoomSearchCursor>,
        order_by: super::models::RoomSearchOrder,
    ) -> Result<(Vec<(Room, i64)>, Option<String>), sqlx::Error>;

    // -- admin search --

    async fn search_all_rooms_admin(
        &self,
        search_term: Option<&str>,
        limit: i64,
        order_by: super::models::RoomSearchOrder,
        cursor: Option<super::models::RoomSearchCursor>,
        is_public: Option<bool>,
        is_encrypted: Option<bool>,
    ) -> Result<(Vec<serde_json::Value>, i64, Option<String>), sqlx::Error>;

    // -- unread counts --

    async fn get_unread_counts(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<super::models::RoomUnreadCounts, sqlx::Error>;

    async fn get_unread_counts_batch(
        &self,
        room_ids: &[String],
        user_id: &str,
    ) -> Result<Vec<super::models::RoomUnreadCounts>, sqlx::Error>;

    // -- cleanup --

    async fn cleanup_abnormal_data(&self, min_age_ms: Option<i64>) -> Result<serde_json::Value, sqlx::Error>;

    // -- power levels (on room due to upsert_power_levels_event existing in room module) --
    // Note: upsert_power_levels_event is on EventStorage, not RoomStorage.
    // It's accessed via event_storage, not room_storage, so it's already on EventRepository.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::room::models;
    use std::sync::Arc;

    /// Minimal in-memory fake implementing the entire `RoomRepository` trait.
    ///
    /// `pool()` returns a lazily-constructed pool handle (never actually
    /// connected) so that the trait contract can be exercised in pure unit
    /// tests without a live Postgres.  All other methods return sensible
    /// empty/default values; specific tests may want to extend the fake with
    /// richer state when needed.
    struct FakeRoomRepository {
        pool: Arc<sqlx::PgPool>,
    }

    impl FakeRoomRepository {
        fn new() -> Self {
            // `connect_lazy` does not perform any I/O — it only validates the
            // URL and stores it for later use, so this is safe in a unit test.
            let pool = sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://unused:unused@localhost:5432/unused")
                .expect("connect_lazy should not perform I/O");
            Self { pool: Arc::new(pool) }
        }
    }

    #[async_trait]
    impl RoomRepository for FakeRoomRepository {
        fn pool(&self) -> &Arc<sqlx::PgPool> {
            &self.pool
        }

        async fn get_room(&self, _room_id: &str) -> Result<Option<Room>, sqlx::Error> {
            Ok(None)
        }

        async fn get_rooms_batch(&self, _room_ids: &[String]) -> Result<Vec<Room>, sqlx::Error> {
            Ok(vec![])
        }

        async fn create_room(
            &self,
            _room_id: &str,
            _creator: &str,
            _join_rule: &str,
            _room_version: &str,
            _is_public: bool,
        ) -> Result<Room, sqlx::Error> {
            Err(sqlx::Error::RowNotFound)
        }

        async fn create_room_in_tx(
            &self,
            _tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
            _room_id: &str,
            _creator: &str,
            _join_rule: &str,
            _version: &str,
            _is_public: bool,
        ) -> Result<Room, sqlx::Error> {
            Err(sqlx::Error::RowNotFound)
        }

        async fn update_room_name(&self, _room_id: &str, _name: &str) -> Result<(), sqlx::Error> {
            Ok(())
        }

        async fn update_room_name_in_tx(
            &self,
            _tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
            _room_id: &str,
            _name: &str,
        ) -> Result<(), sqlx::Error> {
            Ok(())
        }

        async fn update_room_topic(&self, _room_id: &str, _topic: &str) -> Result<(), sqlx::Error> {
            Ok(())
        }

        async fn update_room_topic_in_tx(
            &self,
            _tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
            _room_id: &str,
            _topic: &str,
        ) -> Result<(), sqlx::Error> {
            Ok(())
        }

        async fn update_join_rule_in_tx(
            &self,
            _tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
            _room_id: &str,
            _join_rule: &str,
        ) -> Result<(), sqlx::Error> {
            Ok(())
        }

        async fn set_room_public(&self, _room_id: &str, _is_public: bool) -> Result<(), sqlx::Error> {
            Ok(())
        }

        async fn delete_room(&self, _room_id: &str) -> Result<(), sqlx::Error> {
            Ok(())
        }

        async fn get_public_rooms(&self, _limit: i64) -> Result<Vec<Room>, sqlx::Error> {
            Ok(vec![])
        }

        async fn get_public_rooms_paginated(
            &self,
            _limit: i64,
            _since_ts: Option<i64>,
            _since_room_id: Option<&str>,
        ) -> Result<Vec<Room>, sqlx::Error> {
            Ok(vec![])
        }

        async fn count_public_rooms(&self) -> Result<i64, sqlx::Error> {
            Ok(0)
        }

        async fn get_user_rooms(&self, _user_id: &str) -> Result<Vec<String>, sqlx::Error> {
            Ok(vec![])
        }

        async fn search_room_directory(
            &self,
            _search_term: &str,
            _limit: i64,
        ) -> Result<Vec<Room>, sqlx::Error> {
            Ok(vec![])
        }

        async fn get_room_aliases(&self, _room_id: &str) -> Result<Vec<String>, sqlx::Error> {
            Ok(vec![])
        }

        async fn set_room_alias(
            &self,
            _room_id: &str,
            _alias: &str,
            _created_by: &str,
        ) -> Result<(), sqlx::Error> {
            Ok(())
        }

        async fn get_room_by_alias(&self, _alias: &str) -> Result<Option<String>, sqlx::Error> {
            Ok(None)
        }

        async fn remove_room_alias(&self, _room_id: &str) -> Result<(), sqlx::Error> {
            Ok(())
        }

        async fn remove_room_alias_by_name(&self, _alias: &str) -> Result<(), sqlx::Error> {
            Ok(())
        }

        async fn set_room_directory(&self, _room_id: &str, _is_public: bool) -> Result<(), sqlx::Error> {
            Ok(())
        }

        async fn is_room_in_directory(&self, _room_id: &str) -> Result<bool, sqlx::Error> {
            Ok(false)
        }

        async fn remove_room_directory(&self, _room_id: &str) -> Result<(), sqlx::Error> {
            Ok(())
        }

        async fn set_canonical_alias(&self, _room_id: &str, _alias: Option<&str>) -> Result<(), sqlx::Error> {
            Ok(())
        }

        async fn increment_member_count(&self, _room_id: &str) -> Result<(), sqlx::Error> {
            Ok(())
        }

        async fn decrement_member_count(&self, _room_id: &str) -> Result<(), sqlx::Error> {
            Ok(())
        }

        async fn add_receipt(
            &self,
            _user_id: &str,
            _receipt_user_id: &str,
            _room_id: &str,
            _event_id: &str,
            _receipt_type: &str,
            _data: &serde_json::Value,
        ) -> Result<(), sqlx::Error> {
            Ok(())
        }

        async fn get_receipts(
            &self,
            _room_id: &str,
            _receipt_type: &str,
            _event_id: &str,
        ) -> Result<Vec<models::Receipt>, sqlx::Error> {
            Ok(vec![])
        }

        async fn update_read_marker_with_type(
            &self,
            _room_id: &str,
            _user_id: &str,
            _event_id: &str,
            _marker_type: &str,
        ) -> Result<(), sqlx::Error> {
            Ok(())
        }

        async fn copy_room_state(&self, _source_room_id: &str, _target_room_id: &str) -> Result<(), sqlx::Error> {
            Ok(())
        }

        async fn room_exists(&self, _room_id: &str) -> Result<bool, sqlx::Error> {
            Ok(false)
        }

        async fn get_room_count(&self) -> Result<i64, sqlx::Error> {
            Ok(0)
        }

        async fn get_room_version_only(&self, _room_id: &str) -> Result<Option<String>, sqlx::Error> {
            Ok(None)
        }

        async fn block_room(
            &self,
            _room_id: &str,
            _blocked_at: i64,
            _blocked_by: &str,
            _reason: Option<&str>,
        ) -> Result<(), sqlx::Error> {
            Ok(())
        }

        async fn unblock_room(&self, _room_id: &str) -> Result<(), sqlx::Error> {
            Ok(())
        }

        async fn get_room_block_status(&self, _room_id: &str) -> Result<Option<i64>, sqlx::Error> {
            Ok(None)
        }

        async fn shutdown_room(&self, _room_id: &str) -> Result<(), sqlx::Error> {
            Ok(())
        }

        async fn get_room_stats_overview(&self) -> Result<serde_json::Value, sqlx::Error> {
            Ok(serde_json::json!({}))
        }

        async fn get_single_room_stats(&self, _room_id: &str) -> Result<Option<serde_json::Value>, sqlx::Error> {
            Ok(None)
        }

        async fn get_room_listings_status(&self, _room_id: &str) -> Result<Option<(bool, bool)>, sqlx::Error> {
            Ok(None)
        }

        async fn set_room_public_with_directory(&self, _room_id: &str) -> Result<bool, sqlx::Error> {
            Ok(false)
        }

        async fn set_room_private_with_directory(&self, _room_id: &str) -> Result<bool, sqlx::Error> {
            Ok(false)
        }

        async fn get_user_room_list_summary(
            &self,
            _user_id: &str,
        ) -> Result<Vec<(String, String, String, String)>, sqlx::Error> {
            Ok(vec![])
        }

        async fn get_all_rooms_with_members(
            &self,
            _limit: i64,
            _from: Option<models::RoomSearchCursor>,
            _order_by: models::RoomSearchOrder,
        ) -> Result<(Vec<(Room, i64)>, Option<String>), sqlx::Error> {
            Ok((vec![], None))
        }

        async fn search_all_rooms_admin(
            &self,
            _search_term: Option<&str>,
            _limit: i64,
            _order_by: models::RoomSearchOrder,
            _cursor: Option<models::RoomSearchCursor>,
            _is_public: Option<bool>,
            _is_encrypted: Option<bool>,
        ) -> Result<(Vec<serde_json::Value>, i64, Option<String>), sqlx::Error> {
            Ok((vec![], 0, None))
        }

        async fn get_unread_counts(
            &self,
            _room_id: &str,
            _user_id: &str,
        ) -> Result<models::RoomUnreadCounts, sqlx::Error> {
            Ok(models::RoomUnreadCounts {
                room_id: String::new(),
                highlight_count: 0,
                notification_count: 0,
            })
        }

        async fn get_unread_counts_batch(
            &self,
            _room_ids: &[String],
            _user_id: &str,
        ) -> Result<Vec<models::RoomUnreadCounts>, sqlx::Error> {
            Ok(vec![])
        }

        async fn cleanup_abnormal_data(
            &self,
            _min_age_ms: Option<i64>,
        ) -> Result<serde_json::Value, sqlx::Error> {
            Ok(serde_json::json!({}))
        }
    }

    #[tokio::test]
    async fn test_fake_repo_pool_returns_lazy_handle() {
        let repo = FakeRoomRepository::new();
        // The pool reference is non-null and holds at least one strong ref.
        let pool_ref: &Arc<sqlx::PgPool> = repo.pool();
        assert!(Arc::strong_count(pool_ref) >= 1, "Arc strong count must be >= 1");
        // The pool itself is usable for compile-time queries (we don't execute).
        let _ = pool_ref.connect_options();
    }

    #[tokio::test]
    async fn test_fake_repo_get_room_returns_none() {
        let repo = FakeRoomRepository::new();
        let result = repo.get_room("!nonexistent:example.com").await.expect("get_room should not error");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_fake_repo_room_exists_returns_false() {
        let repo = FakeRoomRepository::new();
        let exists = repo.room_exists("!any:example.com").await.expect("room_exists should not error");
        assert!(!exists);
    }

    #[tokio::test]
    async fn test_fake_repo_count_public_rooms_zero() {
        let repo = FakeRoomRepository::new();
        let count = repo.count_public_rooms().await.expect("count_public_rooms should not error");
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_fake_repo_get_user_rooms_empty() {
        let repo = FakeRoomRepository::new();
        let rooms = repo.get_user_rooms("@alice:example.com").await.expect("get_user_rooms should not error");
        assert!(rooms.is_empty());
    }

    #[tokio::test]
    async fn test_fake_repo_unread_counts_zero() {
        let repo = FakeRoomRepository::new();
        let counts = repo.get_unread_counts("!room:example.com", "@alice:example.com").await.expect("get_unread_counts should not error");
        assert_eq!(counts.notification_count, 0);
        assert_eq!(counts.highlight_count, 0);
    }

    #[tokio::test]
    async fn test_fake_repo_can_be_boxed_as_trait_object() {
        // Verifies the trait is object-safe — a compile-time property
        // that is asserted by actually creating a boxed trait object and
        // dropping it cleanly inside a runtime context (sqlx pool drop
        // requires a Tokio context).
        let repo: Box<dyn RoomRepository> = Box::new(FakeRoomRepository::new());
        let pool_ref = repo.pool();
        assert!(Arc::strong_count(pool_ref) >= 1);
        drop(repo);
    }
}

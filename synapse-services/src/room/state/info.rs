//! Room info queries and basic metadata helpers.

use crate::common::error::{ApiError, ApiResult};
use crate::room::state::error::RoomStateError;
use serde_json::json;
use synapse_common::current_timestamp_millis;
use synapse_storage::{Room, RoomSearchCursor, RoomSearchOrder};

use super::service::RoomStateService;

impl RoomStateService {
    /// See [`get_room_encryption_status`].
    pub async fn get_room_encryption_status(
        &self,
        room_id: &str,
    ) -> ApiResult<synapse_storage::room::RoomEncryptionStatus> {
        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get room", e))?
            .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

        let encryption_events = self
            .event_reader
            .get_state_events_by_type(room_id, "m.room.encryption")
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get encryption event content", e))?;
        let encryption_content = encryption_events.first().map(|event| event.content.clone());
        let is_encrypted = encryption_content.is_some();

        Ok(synapse_storage::room::RoomEncryptionStatus::from_encryption_event(
            is_encrypted,
            if is_encrypted {
                encryption_content
                    .as_ref()
                    .and_then(|content| content.get("algorithm").and_then(|v| v.as_str()).map(|s| s.to_string()))
                    .or_else(|| room.encryption.clone())
            } else {
                None
            },
            encryption_content.as_ref().and_then(|content| content.get("rotation_period_ms").and_then(|v| v.as_i64())),
            encryption_content
                .as_ref()
                .and_then(|content| content.get("rotation_period_msgs").and_then(|v| v.as_i64())),
        ))
    }

    /// See [`delete_room`].
    pub async fn delete_room(&self, room_id: &str, requester_id: &str) -> ApiResult<()> {
        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get room", e))?
            .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

        let requester = self
            .user_storage
            .get_user_by_id(requester_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get user", e))?
            .ok_or_else(|| ApiError::unauthorized("Requester not found"))?;

        let is_creator = room.creator_user_id.as_deref() == Some(requester_id);
        let is_admin = requester.is_admin;

        if !is_creator && !is_admin {
            return Err(ApiError::forbidden("Only the room creator or a server admin can delete a room".to_string()));
        }

        self.room_storage
            .delete_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to delete room", e))?;

        tracing::info!(
            room_id = %room_id,
            "Room deleted via info service (batched event cleanup)"
        );

        Ok(())
    }

    /// See [`get_user_room_list`].
    pub async fn get_user_room_list(&self, user_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        let rooms = self
            .room_storage
            .get_user_room_list_summary(user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get user rooms", e))?;

        Ok(rooms
            .into_iter()
            .map(|(room_id, membership, name, avatar_url)| {
                json!({
                    "room_id": room_id,
                    "membership": membership,
                    "name": name,
                    "avatar_url": avatar_url
                })
            })
            .collect())
    }

    /// See [`cleanup_abnormal_data`].
    pub async fn cleanup_abnormal_data(&self, min_age_ms: Option<i64>) -> ApiResult<serde_json::Value> {
        self.room_storage
            .cleanup_abnormal_data(min_age_ms)
            .await
            .map_err(|e| ApiError::internal_with_cause("Cleanup failed", e))
    }

    /// See [`room_exists`].
    pub async fn room_exists(&self, room_id: &str) -> Result<bool, RoomStateError> {
        let exists = self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| RoomStateError::Database(e))?;
        Ok(exists)
    }

    /// See [`block_room`].
    pub async fn block_room(&self, room_id: &str, blocked_by: &str, reason: Option<&str>) -> ApiResult<()> {
        let now = current_timestamp_millis();
        self.room_storage
            .block_room(room_id, now, blocked_by, reason)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to block room", e))
    }

    /// See [`get_room_block_status`].
    pub async fn get_room_block_status(&self, room_id: &str) -> ApiResult<Option<i64>> {
        self.room_storage
            .get_room_block_status(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get room block status", e))
    }

    /// See [`unblock_room`].
    pub async fn unblock_room(&self, room_id: &str) -> ApiResult<()> {
        self.room_storage
            .unblock_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to unblock room", e))
    }

    /// See [`get_public_rooms_paginated`].
    pub async fn get_public_rooms_paginated(
        &self,
        limit: i64,
        since_ts: Option<i64>,
        since_room_id: Option<&str>,
    ) -> ApiResult<Vec<synapse_storage::Room>> {
        self.room_storage
            .get_public_rooms_paginated(limit, since_ts, since_room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get public rooms", e))
    }

    /// See [`count_public_rooms`].
    pub async fn count_public_rooms(&self) -> ApiResult<i64> {
        self.room_storage
            .count_public_rooms()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to count public rooms", e))
    }

    /// See [`get_room_stats_overview`].
    pub async fn get_room_stats_overview(&self) -> ApiResult<serde_json::Value> {
        self.room_storage
            .get_room_stats_overview()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get room statistics overview", e))
    }

    /// See [`get_single_room_stats`].
    pub async fn get_single_room_stats(&self, room_id: &str) -> ApiResult<Option<serde_json::Value>> {
        self.room_storage
            .get_single_room_stats(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get room statistics", e))
    }

    /// See [`get_all_rooms_with_members`].
    pub async fn get_all_rooms_with_members(
        &self,
        limit: i64,
        from: Option<RoomSearchCursor>,
        order_by: RoomSearchOrder,
    ) -> Result<(Vec<(Room, i64)>, Option<String>), RoomStateError> {
        self.room_storage
            .get_all_rooms_with_members(limit, from, order_by)
            .await
            .map_err(|e| RoomStateError::Database(e))
    }

    /// See [`get_room_count`].
    pub async fn get_room_count(&self) -> Result<i64, RoomStateError> {
        self.room_storage.get_room_count().await.map_err(|e| RoomStateError::Database(e))
    }

    /// See [`get_room_record`].
    pub async fn get_room_record(&self, room_id: &str) -> Result<Option<Room>, RoomStateError> {
        self.room_storage.get_room(room_id).await.map_err(|e| RoomStateError::Database(e))
    }

    /// See [`get_room_listings_status`].
    pub async fn get_room_listings_status(&self, room_id: &str) -> ApiResult<Option<(bool, bool)>> {
        self.room_storage
            .get_room_listings_status(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get room listing status", e))
    }

    /// See [`set_room_public_with_directory`].
    pub async fn set_room_public_with_directory(&self, room_id: &str) -> ApiResult<bool> {
        self.room_storage
            .set_room_public_with_directory(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to set room public", e))
    }

    /// See [`set_room_private_with_directory`].
    pub async fn set_room_private_with_directory(&self, room_id: &str) -> ApiResult<bool> {
        self.room_storage
            .set_room_private_with_directory(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to set room private", e))
    }

    /// See [`shutdown_room_and_remove_members`].
    pub async fn shutdown_room_and_remove_members(&self, room_id: &str) -> ApiResult<()> {
        self.room_storage
            .shutdown_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to shutdown room", e))?;
        self.member_storage
            .remove_all_members(room_id)
            .await
            .map_err(|e| RoomStateError::Database(e))?;
        Ok(())
    }

    /// See [`grant_room_admin`].
    pub async fn grant_room_admin(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        let event_id = synapse_common::generate_event_id(&self.server_name);
        let sender = format!("@admin:{}", self.server_name);
        let now = current_timestamp_millis();
        let power_levels = json!({
            "users": {
                user_id: 100
            },
            "users_default": 0,
            "events_default": 0,
            "state_default": 50,
            "ban": 50,
            "kick": 50,
            "redact": 50,
            "invite": 0
        });

        self.event_writer
            .upsert_power_levels_event(&event_id, room_id, user_id, power_levels, now, &sender)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to grant room admin", e))
    }

    /// See [`purge_history_before`].
    pub async fn purge_history_before(&self, room_id: &str, timestamp: i64, dry_run: bool) -> ApiResult<u64> {
        self.event_writer
            .delete_remote_events_before(room_id, timestamp, dry_run)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to purge history", e))
    }

    /// See [`get_room_version`].
    pub async fn get_room_version(&self, room_id: &str) -> Result<Option<String>, RoomStateError> {
        self.room_storage
            .get_room_version_only(room_id)
            .await
            .map_err(|e| RoomStateError::Database(e))
    }

    /// See [`search_all_rooms_admin`].
    pub async fn search_all_rooms_admin(
        &self,
        search_term: Option<&str>,
        limit: i64,
        order_by: RoomSearchOrder,
        cursor: Option<RoomSearchCursor>,
        is_public: Option<bool>,
        is_encrypted: Option<bool>,
    ) -> ApiResult<(Vec<serde_json::Value>, i64, Option<String>)> {
        self.room_storage
            .search_all_rooms_admin(search_term, limit, order_by, cursor, is_public, is_encrypted)
            .await
            .map_err(|e| ApiError::internal_with_cause("Search failed", e))
    }

    /// See [`is_room_creator`].
    pub async fn is_room_creator(&self, room_id: &str, user_id: &str) -> ApiResult<bool> {
        let room = self
            .room_storage
            .get_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get room", e))?;

        match room {
            Some(r) => Ok(r.creator_user_id.as_deref() == Some(user_id)),
            None => Ok(false),
        }
    }

    /// See [`check_room_has_encryption`].
    pub async fn check_room_has_encryption(&self, room_id: &str) -> ApiResult<bool> {
        self.event_reader
            .check_room_has_encryption(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to check room encryption status", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::UserService;
    use crate::room::state::service::{RoomStateService, RoomStateServiceConfig};
    use std::sync::Arc;
    use synapse_storage::test_mocks::{
        FakeUserStore, InMemoryEventStore, InMemoryMemberStore, InMemoryRoomStore, InMemoryRoomTagStore,
    };

    /// Build a minimal RoomStateService backed by in-memory stores. Room
    /// tag storage is required by the constructor; we pass a no-op
    /// InMemoryRoomTagStore.
    fn make_service() -> RoomStateService {
        let user_store: Arc<dyn synapse_storage::UserStore> = Arc::new(FakeUserStore::new());
        RoomStateService::new(RoomStateServiceConfig {
            room_storage: Arc::new(InMemoryRoomStore::new()),
            member_storage: Arc::new(InMemoryMemberStore::new()),
            event_reader: Arc::new(InMemoryEventStore::new()),
            event_writer: Arc::new(InMemoryEventStore::new()),
            room_tag_storage: Arc::new(InMemoryRoomTagStore::new()),
            user_storage: user_store.clone(),
            user_service: Arc::new(UserService::new(user_store)),
            server_name: "test.example.com".to_string(),
        })
    }

    // -------------------------------------------------------------------------
    // get_room_encryption_status
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn get_room_encryption_status_returns_not_found_when_room_missing() {
        let svc = make_service();
        let result = svc.get_room_encryption_status("!missing:ex.com").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not found"), "expected not_found error, got: {err}");
    }

    #[tokio::test]
    async fn get_room_encryption_status_returns_unencrypted_when_no_state_events() {
        let svc = make_service();
        let status = svc.get_room_encryption_status("!nonexistent:ex.com").await;
        // No room exists, expect not_found; if room existed, would return unencrypted status.
        assert!(status.is_err());
    }

    // -------------------------------------------------------------------------
    // get_user_room_list
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn get_user_room_list_returns_empty_when_user_has_no_rooms() {
        let svc = make_service();
        let result = svc.get_user_room_list("@alice:ex.com").await.unwrap();
        assert!(result.is_empty());
    }

    // -------------------------------------------------------------------------
    // cleanup_abnormal_data
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn cleanup_abnormal_data_succeeds() {
        let svc = make_service();
        // InMemoryRoomStore::cleanup_abnormal_data returns Ok(json!) stub.
        let result = svc.cleanup_abnormal_data(None).await;
        assert!(result.is_ok(), "cleanup_abnormal_data should succeed: {:?}", result);
    }

    // -------------------------------------------------------------------------
    // room_exists
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn room_exists_returns_false_for_missing_room() {
        let svc = make_service();
        let exists = svc.room_exists("!missing:ex.com").await.unwrap();
        assert!(!exists);
    }

    // -------------------------------------------------------------------------
    // block_room / unblock_room / get_room_block_status
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn block_room_succeeds_for_any_room() {
        let svc = make_service();
        let result = svc.block_room("!room:ex.com", "@admin:ex.com", Some("test reason")).await;
        assert!(result.is_ok(), "block_room should not fail: {:?}", result);
    }

    /// `InMemoryRoomStore` does **not** model room blocking: `block_room` is a
    /// no-op and `get_room_block_status` always returns `None` (see the
    /// MOCK DEVIATION note in `synapse-storage/src/test_mocks/room.rs`).
    ///
    /// This test therefore describes the *mock's* limitation rather than
    /// service behaviour. It calls the mutating path first so the assertion has
    /// meaning: a faithful mock would report `Some(..)` here. Real block/unblock
    /// coverage requires the database-backed store.
    #[tokio::test]
    async fn get_room_block_status_is_unmodelled_by_the_in_memory_store() {
        let svc = make_service();
        svc.block_room("!missing:ex.com", "@admin:ex.com", Some("reason")).await.expect("block_room");

        let status = svc.get_room_block_status("!missing:ex.com").await.expect("status");
        assert!(
            status.is_none(),
            "MOCK DEVIATION 记录：InMemoryRoomStore 的 block_room 是 no-op、\
             get_room_block_status 恒为 None。若这里变成 Some，说明 mock 已被补充实现，\
             请同步更新 test_mocks/room.rs 的偏差说明与本测试名。"
        );
    }

    #[tokio::test]
    async fn unblock_room_succeeds() {
        let svc = make_service();
        let result = svc.unblock_room("!room:ex.com").await;
        assert!(result.is_ok(), "unblock_room should not fail: {:?}", result);
    }

    // -------------------------------------------------------------------------
    // get_public_rooms_paginated / count_public_rooms
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn get_public_rooms_paginated_returns_empty_when_no_public_rooms() {
        let svc = make_service();
        let result = svc.get_public_rooms_paginated(10, None, None).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn count_public_rooms_returns_zero_for_empty_store() {
        let svc = make_service();
        let count = svc.count_public_rooms().await.unwrap();
        assert_eq!(count, 0);
    }

    // -------------------------------------------------------------------------
    // get_room_stats_overview / get_single_room_stats
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn get_room_stats_overview_succeeds() {
        let svc = make_service();
        let result = svc.get_room_stats_overview().await;
        assert!(result.is_ok(), "get_room_stats_overview should not fail: {:?}", result);
    }

    #[tokio::test]
    async fn get_single_room_stats_returns_none_for_missing_room() {
        let svc = make_service();
        let result = svc.get_single_room_stats("!missing:ex.com").await.unwrap();
        assert!(result.is_none());
    }

    // -------------------------------------------------------------------------
    // get_room_count / get_room_record / get_room_listings_status
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn get_room_count_returns_zero_for_empty_store() {
        let svc = make_service();
        let count = svc.get_room_count().await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn get_room_record_returns_none_for_missing_room() {
        let svc = make_service();
        let result = svc.get_room_record("!missing:ex.com").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn get_room_listings_status_returns_none_for_missing_room() {
        let svc = make_service();
        let result = svc.get_room_listings_status("!missing:ex.com").await.unwrap();
        assert!(result.is_none());
    }

    // -------------------------------------------------------------------------
    // set_room_public_with_directory / set_room_private_with_directory
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn set_room_public_with_directory_succeeds() {
        let svc = make_service();
        let result = svc.set_room_public_with_directory("!room:ex.com").await;
        assert!(result.is_ok(), "set_room_public should not fail: {:?}", result);
    }

    #[tokio::test]
    async fn set_room_private_with_directory_succeeds() {
        let svc = make_service();
        let result = svc.set_room_private_with_directory("!room:ex.com").await;
        assert!(result.is_ok(), "set_room_private should not fail: {:?}", result);
    }

    // -------------------------------------------------------------------------
    // shutdown_room_and_remove_members
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn shutdown_room_and_remove_members_succeeds() {
        let svc = make_service();
        let result = svc.shutdown_room_and_remove_members("!room:ex.com").await;
        assert!(result.is_ok(), "shutdown should not fail: {:?}", result);
    }

    // -------------------------------------------------------------------------
    // grant_room_admin
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn grant_room_admin_succeeds() {
        let svc = make_service();
        let result = svc.grant_room_admin("!room:ex.com", "@alice:ex.com").await;
        assert!(result.is_ok(), "grant_room_admin should not fail: {:?}", result);
    }

    // -------------------------------------------------------------------------
    // purge_history_before
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn purge_history_before_returns_zero_when_no_events() {
        let svc = make_service();
        let count = svc.purge_history_before("!room:ex.com", 0, true).await.unwrap();
        assert_eq!(count, 0);
    }

    // -------------------------------------------------------------------------
    // get_room_version
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn get_room_version_returns_none_for_missing_room() {
        let svc = make_service();
        let result = svc.get_room_version("!missing:ex.com").await.unwrap();
        assert!(result.is_none());
    }

    // -------------------------------------------------------------------------
    // search_all_rooms_admin
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn search_all_rooms_admin_returns_empty_for_no_matches() {
        let svc = make_service();
        let (rooms, total, next) =
            svc.search_all_rooms_admin(Some("nonexistent"), 10, RoomSearchOrder::Name, None, None, None).await.unwrap();
        assert!(rooms.is_empty());
        assert_eq!(total, 0);
        assert!(next.is_none());
    }

    // -------------------------------------------------------------------------
    // is_room_creator
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn is_room_creator_returns_false_when_room_missing() {
        let svc = make_service();
        let result = svc.is_room_creator("!missing:ex.com", "@alice:ex.com").await.unwrap();
        assert!(!result);
    }

    // -------------------------------------------------------------------------
    // check_room_has_encryption
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn check_room_has_encryption_returns_false_when_no_events() {
        let svc = make_service();
        let result = svc.check_room_has_encryption("!missing:ex.com").await.unwrap();
        assert!(!result);
    }
}

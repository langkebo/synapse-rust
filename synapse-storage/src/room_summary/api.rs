use super::models::*;
use super::RoomSummaryStorage;
use async_trait::async_trait;
use std::collections::HashMap;

/// The `RoomSummaryStoreApi` trait.
#[async_trait]
pub trait RoomSummaryStoreApi: Send + Sync {
    /// See [`create_summary`].
    async fn create_summary(&self, request: CreateRoomSummaryRequest) -> Result<RoomSummary, sqlx::Error>;
    /// See [`get_summary`].
    async fn get_summary(&self, room_id: &str) -> Result<Option<RoomSummary>, sqlx::Error>;
    /// See [`update_summary`].
    async fn update_summary(
        &self,
        room_id: &str,
        request: UpdateRoomSummaryRequest,
    ) -> Result<RoomSummary, sqlx::Error>;
    /// See [`set_canonical_alias`].
    async fn set_canonical_alias(
        &self,
        room_id: &str,
        canonical_alias: Option<&str>,
    ) -> Result<RoomSummary, sqlx::Error>;
    /// See [`delete_summary`].
    async fn delete_summary(&self, room_id: &str) -> Result<(), sqlx::Error>;
    /// See [`get_summaries_by_ids`].
    async fn get_summaries_by_ids(&self, room_ids: &[String]) -> Result<Vec<RoomSummary>, sqlx::Error>;
    /// See [`get_summaries_for_user`].
    async fn get_summaries_for_user(&self, user_id: &str) -> Result<Vec<RoomSummary>, sqlx::Error>;
    /// See [`get_heroes`].
    async fn get_heroes(&self, room_id: &str, limit: i64) -> Result<Vec<RoomSummaryMember>, sqlx::Error>;
    /// See [`get_heroes_batch`].
    async fn get_heroes_batch(
        &self,
        room_ids: &[String],
        limit: i64,
    ) -> Result<HashMap<String, Vec<RoomSummaryMember>>, sqlx::Error>;
    /// See [`add_member`].
    async fn add_member(&self, request: CreateSummaryMemberRequest) -> Result<RoomSummaryMember, sqlx::Error>;
    /// DB-06 / P0-2: transactional variant of `add_member`. Used by
    /// `MembershipService::add_member` to keep the `room_memberships` and
    /// `room_summary_members` writes atomic in the no-caller-tx path.
    async fn add_member_in_tx(
        &self,
        request: CreateSummaryMemberRequest,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<RoomSummaryMember, sqlx::Error>;
    /// See [`add_members_batch`].
    async fn add_members_batch(
        &self,
        room_id: &str,
        members: Vec<CreateSummaryMemberRequest>,
    ) -> Result<usize, sqlx::Error>;
    /// See [`update_member`].
    async fn update_member(
        &self,
        room_id: &str,
        user_id: &str,
        request: UpdateSummaryMemberRequest,
    ) -> Result<RoomSummaryMember, sqlx::Error>;
    /// See [`remove_member`].
    async fn remove_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error>;
    /// See [`get_members`].
    async fn get_members(&self, room_id: &str) -> Result<Vec<RoomSummaryMember>, sqlx::Error>;
    /// See [`set_state`].
    async fn set_state(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
        event_id: Option<&str>,
        content: serde_json::Value,
    ) -> Result<RoomSummaryState, sqlx::Error>;
    /// See [`set_states_batch`].
    async fn set_states_batch(&self, room_id: &str, entries: &[RoomSummaryStateEntry]) -> Result<u64, sqlx::Error>;
    /// See [`get_state`].
    async fn get_state(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> Result<Option<RoomSummaryState>, sqlx::Error>;
    /// See [`get_all_state`].
    async fn get_all_state(&self, room_id: &str) -> Result<Vec<RoomSummaryState>, sqlx::Error>;
    /// See [`get_stats`].
    async fn get_stats(&self, room_id: &str) -> Result<Option<RoomSummaryStats>, sqlx::Error>;
    /// See [`update_stats`].
    async fn update_stats(
        &self,
        room_id: &str,
        total_events: i64,
        total_state_events: i64,
        total_messages: i64,
        total_media: i64,
        storage_size: i64,
    ) -> Result<RoomSummaryStats, sqlx::Error>;
    /// See [`queue_update`].
    async fn queue_update(
        &self,
        room_id: &str,
        event_id: &str,
        event_type: &str,
        state_key: Option<&str>,
        priority: i32,
    ) -> Result<(), sqlx::Error>;
    /// See [`get_pending_updates`].
    async fn get_pending_updates(&self, limit: i64) -> Result<Vec<RoomSummaryUpdateQueueItem>, sqlx::Error>;
    /// See [`mark_update_processed`].
    async fn mark_update_processed(&self, id: i64) -> Result<(), sqlx::Error>;
    /// See [`mark_update_failed`].
    async fn mark_update_failed(&self, id: i64, error: &str) -> Result<(), sqlx::Error>;
    /// See [`increment_unread_notifications`].
    async fn increment_unread_notifications(&self, room_id: &str, highlight: bool) -> Result<(), sqlx::Error>;
    /// See [`clear_unread_notifications`].
    async fn clear_unread_notifications(&self, room_id: &str) -> Result<(), sqlx::Error>;
    /// See [`get_hero_candidates`].
    async fn get_hero_candidates(&self, room_id: &str, limit: i64) -> Result<Vec<RoomSummaryMember>, sqlx::Error>;
    /// See [`set_hero_members`].
    async fn set_hero_members(&self, room_id: &str, hero_user_ids: &[String]) -> Result<(), sqlx::Error>;
}

#[async_trait]
impl RoomSummaryStoreApi for RoomSummaryStorage {
    async fn create_summary(&self, request: CreateRoomSummaryRequest) -> Result<RoomSummary, sqlx::Error> {
        self.create_summary(request).await
    }
    async fn get_summary(&self, room_id: &str) -> Result<Option<RoomSummary>, sqlx::Error> {
        self.get_summary(room_id).await
    }
    async fn update_summary(
        &self,
        room_id: &str,
        request: UpdateRoomSummaryRequest,
    ) -> Result<RoomSummary, sqlx::Error> {
        self.update_summary(room_id, request).await
    }
    async fn set_canonical_alias(
        &self,
        room_id: &str,
        canonical_alias: Option<&str>,
    ) -> Result<RoomSummary, sqlx::Error> {
        self.set_canonical_alias(room_id, canonical_alias).await
    }
    async fn delete_summary(&self, room_id: &str) -> Result<(), sqlx::Error> {
        self.delete_summary(room_id).await
    }
    async fn get_summaries_by_ids(&self, room_ids: &[String]) -> Result<Vec<RoomSummary>, sqlx::Error> {
        self.get_summaries_by_ids(room_ids).await
    }
    async fn get_summaries_for_user(&self, user_id: &str) -> Result<Vec<RoomSummary>, sqlx::Error> {
        self.get_summaries_for_user(user_id).await
    }
    async fn get_heroes(&self, room_id: &str, limit: i64) -> Result<Vec<RoomSummaryMember>, sqlx::Error> {
        self.get_heroes(room_id, limit).await
    }
    async fn get_heroes_batch(
        &self,
        room_ids: &[String],
        limit: i64,
    ) -> Result<HashMap<String, Vec<RoomSummaryMember>>, sqlx::Error> {
        self.get_heroes_batch(room_ids, limit).await
    }
    async fn add_member(&self, request: CreateSummaryMemberRequest) -> Result<RoomSummaryMember, sqlx::Error> {
        self.add_member(request).await
    }
    async fn add_member_in_tx(
        &self,
        request: CreateSummaryMemberRequest,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<RoomSummaryMember, sqlx::Error> {
        self.add_member_in_tx(request, tx).await
    }
    async fn add_members_batch(
        &self,
        room_id: &str,
        members: Vec<CreateSummaryMemberRequest>,
    ) -> Result<usize, sqlx::Error> {
        self.add_members_batch(room_id, members).await
    }
    async fn update_member(
        &self,
        room_id: &str,
        user_id: &str,
        request: UpdateSummaryMemberRequest,
    ) -> Result<RoomSummaryMember, sqlx::Error> {
        self.update_member(room_id, user_id, request).await
    }
    async fn remove_member(&self, room_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        self.remove_member(room_id, user_id).await
    }
    async fn get_members(&self, room_id: &str) -> Result<Vec<RoomSummaryMember>, sqlx::Error> {
        self.get_members(room_id).await
    }
    async fn set_state(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
        event_id: Option<&str>,
        content: serde_json::Value,
    ) -> Result<RoomSummaryState, sqlx::Error> {
        self.set_state(room_id, event_type, state_key, event_id, content).await
    }
    async fn set_states_batch(&self, room_id: &str, entries: &[RoomSummaryStateEntry]) -> Result<u64, sqlx::Error> {
        self.set_states_batch(room_id, entries).await
    }
    async fn get_state(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> Result<Option<RoomSummaryState>, sqlx::Error> {
        self.get_state(room_id, event_type, state_key).await
    }
    async fn get_all_state(&self, room_id: &str) -> Result<Vec<RoomSummaryState>, sqlx::Error> {
        self.get_all_state(room_id).await
    }
    async fn get_stats(&self, room_id: &str) -> Result<Option<RoomSummaryStats>, sqlx::Error> {
        self.get_stats(room_id).await
    }
    async fn update_stats(
        &self,
        room_id: &str,
        total_events: i64,
        total_state_events: i64,
        total_messages: i64,
        total_media: i64,
        storage_size: i64,
    ) -> Result<RoomSummaryStats, sqlx::Error> {
        self.update_stats(room_id, total_events, total_state_events, total_messages, total_media, storage_size).await
    }
    async fn queue_update(
        &self,
        room_id: &str,
        event_id: &str,
        event_type: &str,
        state_key: Option<&str>,
        priority: i32,
    ) -> Result<(), sqlx::Error> {
        self.queue_update(room_id, event_id, event_type, state_key, priority).await
    }
    async fn get_pending_updates(&self, limit: i64) -> Result<Vec<RoomSummaryUpdateQueueItem>, sqlx::Error> {
        self.get_pending_updates(limit).await
    }
    async fn mark_update_processed(&self, id: i64) -> Result<(), sqlx::Error> {
        self.mark_update_processed(id).await
    }
    async fn mark_update_failed(&self, id: i64, error: &str) -> Result<(), sqlx::Error> {
        self.mark_update_failed(id, error).await
    }
    async fn increment_unread_notifications(&self, room_id: &str, highlight: bool) -> Result<(), sqlx::Error> {
        self.increment_unread_notifications(room_id, highlight).await
    }
    async fn clear_unread_notifications(&self, room_id: &str) -> Result<(), sqlx::Error> {
        self.clear_unread_notifications(room_id).await
    }
    async fn get_hero_candidates(&self, room_id: &str, limit: i64) -> Result<Vec<RoomSummaryMember>, sqlx::Error> {
        self.get_hero_candidates(room_id, limit).await
    }
    async fn set_hero_members(&self, room_id: &str, hero_user_ids: &[String]) -> Result<(), sqlx::Error> {
        self.set_hero_members(room_id, hero_user_ids).await
    }
}

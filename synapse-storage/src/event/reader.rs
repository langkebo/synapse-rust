use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::models::*;

/// Read-only interface for event queries.
#[async_trait]
pub trait EventReader: Send + Sync {
    /// Returns a reference to the database connection pool.
    /// In-memory implementations may return `unimplemented!()`.
    fn pool(&self) -> &Arc<sqlx::PgPool>;

    // ── single-event ──────────────────────────────────────────────────

    async fn get_event(&self, event_id: &str) -> Result<Option<RoomEvent>, sqlx::Error>;

    // ── bulk-read ─────────────────────────────────────────────────────

    async fn get_room_events(&self, room_id: &str, limit: i64) -> Result<Vec<RoomEvent>, sqlx::Error>;

    async fn get_room_events_paginated(
        &self,
        room_id: &str,
        from: Option<i64>,
        limit: i64,
        direction: &str,
    ) -> Result<Vec<RoomEvent>, sqlx::Error>;

    async fn get_room_events_batch(
        &self,
        room_ids: &[String],
        limit_per_room: i64,
    ) -> Result<HashMap<String, Vec<RoomEvent>>, sqlx::Error>;

    async fn get_room_events_batch_since(
        &self,
        room_ids: &[String],
        since: SinceFilter,
        limit_per_room: i64,
    ) -> Result<HashMap<String, Vec<RoomEvent>>, sqlx::Error>;

    async fn get_room_events_batch_since_filtered(
        &self,
        room_ids: &[String],
        since: SinceFilter,
        limit_per_room: i64,
        filter: &EventQueryFilter,
    ) -> Result<HashMap<String, Vec<RoomEvent>>, sqlx::Error>;

    // ── state events ──────────────────────────────────────────────────

    async fn get_state_event(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> Result<Option<StateEvent>, sqlx::Error>;

    async fn get_state_events(&self, room_id: &str) -> Result<Vec<StateEvent>, sqlx::Error>;

    async fn get_state_events_by_type(&self, room_id: &str, event_type: &str) -> Result<Vec<StateEvent>, sqlx::Error>;

    async fn get_state_events_at_or_before(
        &self,
        room_id: &str,
        origin_server_ts: i64,
    ) -> Result<Vec<StateEvent>, sqlx::Error>;

    // ── helpers ───────────────────────────────────────────────────────

    async fn get_events_map(&self, event_ids: &[String]) -> Result<HashMap<String, RoomEvent>, sqlx::Error>;

    async fn get_max_origin_server_ts_for_room(&self, room_id: &str) -> Result<i64, sqlx::Error>;

    async fn get_latest_event_ids_in_room(&self, room_id: &str, limit: i64) -> Result<Vec<String>, sqlx::Error>;

    async fn count_room_events_by_status(&self, room_id: &str, status: &str) -> Result<i64, sqlx::Error>;

    // ── ephemeral ───────────────────────────────────────────────────────

    async fn get_ephemeral_events(
        &self,
        room_id: &str,
        now: i64,
        limit: i64,
    ) -> Result<Vec<RoomEphemeralEvent>, sqlx::Error>;

    async fn get_ephemeral_events_batch(
        &self,
        room_ids: &[String],
        now: i64,
        limit: i64,
    ) -> Result<HashMap<String, Vec<RoomEphemeralEvent>>, sqlx::Error>;

    // ── state-batch ─────────────────────────────────────────────────────

    async fn get_state_events_batch(
        &self,
        room_ids: &[String],
    ) -> Result<HashMap<String, Vec<StateEvent>>, sqlx::Error>;

    async fn get_state_events_by_type_batch(
        &self,
        room_ids: &[String],
        event_type: &str,
    ) -> Result<HashMap<String, Vec<StateEvent>>, sqlx::Error>;

    async fn get_state_events_since_batch(
        &self,
        room_ids: &[String],
        since: SinceFilter,
    ) -> Result<HashMap<String, Vec<StateEvent>>, sqlx::Error>;

    async fn get_membership_state_keys_since_batch(
        &self,
        room_ids: &[String],
        since: SinceFilter,
    ) -> Result<HashMap<String, HashSet<String>>, sqlx::Error>;

    async fn get_state_change_timestamps_batch(
        &self,
        room_ids: &[String],
        since: SinceFilter,
    ) -> Result<HashMap<String, i64>, sqlx::Error>;

    // ── filtered-batch ──────────────────────────────────────────────────

    async fn get_room_events_batch_filtered(
        &self,
        room_ids: &[String],
        limit_per_room: i64,
        filter: &EventQueryFilter,
    ) -> Result<HashMap<String, Vec<RoomEvent>>, sqlx::Error>;

    // ── misc ────────────────────────────────────────────────────────────

    async fn has_room_events_since(&self, room_ids: &[String], since: i64) -> Result<bool, sqlx::Error>;

    // ── graph / dag ──────────────────────────────────────────────────────

    async fn find_missing_event_ids(&self, event_ids: &[String]) -> Result<Vec<String>, sqlx::Error>;

    async fn get_missing_events_between(
        &self,
        room_id: &str,
        earliest_events: &[String],
        latest_events: &[String],
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error>;

    async fn get_forward_extremities_count(&self, room_id: &str) -> Result<i64, sqlx::Error>;

    // ── context / pagination ────────────────────────────────────────────

    async fn find_event_id_by_timestamp(
        &self,
        room_id: &str,
        ts: i64,
        forward: bool,
    ) -> Result<Option<(String, i64)>, sqlx::Error>;

    async fn get_events_before_context(
        &self,
        room_id: &str,
        before_ts: i64,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error>;

    async fn get_events_after_context(
        &self,
        room_id: &str,
        after_ts: i64,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error>;

    // ── by-type / pending / counts ──────────────────────────────────────

    async fn get_room_events_by_type(
        &self,
        room_id: &str,
        event_type: &str,
        limit: i64,
    ) -> Result<Vec<RoomEvent>, sqlx::Error>;

    async fn get_pending_room_events(&self, room_id: &str, limit: i64) -> Result<Vec<RoomEvent>, sqlx::Error>;

    async fn get_daily_message_count(&self) -> Result<i64, sqlx::Error>;

    // ── signatures / search / encryption ──────────────────────────────────

    async fn get_event_signatures(&self, event_id: &str) -> Result<Vec<EventSignature>, sqlx::Error>;

    async fn search_room_messages_admin(
        &self,
        room_id: &str,
        search_pattern: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error>;

    async fn check_room_has_encryption(&self, room_id: &str) -> Result<bool, sqlx::Error>;
}

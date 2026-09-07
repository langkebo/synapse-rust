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

    /// See [`get_event`].
    async fn get_event(&self, event_id: &str) -> Result<Option<RoomEvent>, sqlx::Error>;

    // ── bulk-read ─────────────────────────────────────────────────────

    /// See [`get_room_events`].
    async fn get_room_events(&self, room_id: &str, limit: i64) -> Result<Vec<RoomEvent>, sqlx::Error>;

    /// See [`get_room_events_paginated`].
    async fn get_room_events_paginated(
        &self,
        room_id: &str,
        from: Option<i64>,
        limit: i64,
        direction: &str,
    ) -> Result<Vec<RoomEvent>, sqlx::Error>;

    /// ISSUE-06: composite-cursor pagination. `(origin_server_ts,
    /// stream_ordering)` tuple locates the page boundary exactly, so
    /// same-millisecond events are neither skipped nor duplicated.
    /// `Some((ts, None))` keeps the legacy strict-timestamp semantics.
    async fn get_room_events_paginated_cursor(
        &self,
        room_id: &str,
        from: Option<(i64, Option<i64>)>,
        limit: i64,
        direction: &str,
    ) -> Result<Vec<RoomEvent>, sqlx::Error>;

    /// See [`get_room_events_batch`].
    async fn get_room_events_batch(
        &self,
        room_ids: &[String],
        limit_per_room: i64,
    ) -> Result<HashMap<String, Vec<RoomEvent>>, sqlx::Error>;

    /// See [`get_room_events_batch_since`].
    async fn get_room_events_batch_since(
        &self,
        room_ids: &[String],
        since: SinceFilter,
        limit_per_room: i64,
    ) -> Result<HashMap<String, Vec<RoomEvent>>, sqlx::Error>;

    /// See [`get_room_events_batch_since_filtered`].
    async fn get_room_events_batch_since_filtered(
        &self,
        room_ids: &[String],
        since: SinceFilter,
        limit_per_room: i64,
        filter: &EventQueryFilter,
    ) -> Result<HashMap<String, Vec<RoomEvent>>, sqlx::Error>;

    // ── state events ──────────────────────────────────────────────────

    /// See [`get_state_event`].
    async fn get_state_event(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> Result<Option<StateEvent>, sqlx::Error>;

    /// See [`get_state_events`].
    async fn get_state_events(&self, room_id: &str) -> Result<Vec<StateEvent>, sqlx::Error>;

    /// See [`get_state_events_by_type`].
    async fn get_state_events_by_type(&self, room_id: &str, event_type: &str) -> Result<Vec<StateEvent>, sqlx::Error>;

    /// See [`get_state_events_at_or_before`].
    async fn get_state_events_at_or_before(
        &self,
        room_id: &str,
        origin_server_ts: i64,
    ) -> Result<Vec<StateEvent>, sqlx::Error>;

    // ── helpers ───────────────────────────────────────────────────────

    /// See [`get_events_map`].
    async fn get_events_map(&self, event_ids: &[String]) -> Result<HashMap<String, RoomEvent>, sqlx::Error>;

    /// See [`get_max_origin_server_ts_for_room`].
    async fn get_max_origin_server_ts_for_room(&self, room_id: &str) -> Result<i64, sqlx::Error>;

    /// See [`get_latest_event_ids_in_room`].
    async fn get_latest_event_ids_in_room(&self, room_id: &str, limit: i64) -> Result<Vec<String>, sqlx::Error>;

    /// See [`count_room_events_by_status`].
    async fn count_room_events_by_status(&self, room_id: &str, status: &str) -> Result<i64, sqlx::Error>;

    // ── ephemeral ───────────────────────────────────────────────────────

    /// See [`get_ephemeral_events`].
    async fn get_ephemeral_events(
        &self,
        room_id: &str,
        now: i64,
        limit: i64,
    ) -> Result<Vec<RoomEphemeralEvent>, sqlx::Error>;

    /// See [`get_ephemeral_events_batch`].
    async fn get_ephemeral_events_batch(
        &self,
        room_ids: &[String],
        now: i64,
        limit: i64,
    ) -> Result<HashMap<String, Vec<RoomEphemeralEvent>>, sqlx::Error>;

    // ── state-batch ─────────────────────────────────────────────────────

    /// See [`get_state_events_batch`].
    async fn get_state_events_batch(
        &self,
        room_ids: &[String],
    ) -> Result<HashMap<String, Vec<StateEvent>>, sqlx::Error>;

    /// See [`get_state_events_by_type_batch`].
    async fn get_state_events_by_type_batch(
        &self,
        room_ids: &[String],
        event_type: &str,
    ) -> Result<HashMap<String, Vec<StateEvent>>, sqlx::Error>;

    /// See [`get_state_events_since_batch`].
    async fn get_state_events_since_batch(
        &self,
        room_ids: &[String],
        since: SinceFilter,
    ) -> Result<HashMap<String, Vec<StateEvent>>, sqlx::Error>;

    /// See [`get_membership_state_keys_since_batch`].
    async fn get_membership_state_keys_since_batch(
        &self,
        room_ids: &[String],
        since: SinceFilter,
    ) -> Result<HashMap<String, HashSet<String>>, sqlx::Error>;

    /// See [`get_state_change_timestamps_batch`].
    async fn get_state_change_timestamps_batch(
        &self,
        room_ids: &[String],
        since: SinceFilter,
    ) -> Result<HashMap<String, i64>, sqlx::Error>;

    // ── filtered-batch ──────────────────────────────────────────────────

    /// See [`get_room_events_batch_filtered`].
    async fn get_room_events_batch_filtered(
        &self,
        room_ids: &[String],
        limit_per_room: i64,
        filter: &EventQueryFilter,
    ) -> Result<HashMap<String, Vec<RoomEvent>>, sqlx::Error>;

    // ── misc ────────────────────────────────────────────────────────────

    /// See [`has_room_events_since`].
    async fn has_room_events_since(&self, room_ids: &[String], since: i64) -> Result<bool, sqlx::Error>;

    // ── unread counts / room state copy ────────────────────────────────
    //
    // These queries read from the `events` table and were moved here from
    // `RoomStorage` to enforce the storage-layer boundary (project rules §7.1).

    /// See [`get_unread_counts`].
    async fn get_unread_counts(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<crate::room::RoomUnreadCounts, sqlx::Error>;

    /// See [`get_unread_counts_batch`].
    async fn get_unread_counts_batch(
        &self,
        room_ids: &[String],
        user_id: &str,
    ) -> Result<Vec<crate::room::RoomUnreadCounts>, sqlx::Error>;

    /// See [`copy_room_state`].
    async fn copy_room_state(&self, source_room_id: &str, target_room_id: &str) -> Result<(), sqlx::Error>;

    // ── graph / dag ──────────────────────────────────────────────────────

    /// See [`find_missing_event_ids`].
    async fn find_missing_event_ids(&self, event_ids: &[String]) -> Result<Vec<String>, sqlx::Error>;

    /// See [`get_missing_events_between`].
    async fn get_missing_events_between(
        &self,
        room_id: &str,
        earliest_events: &[String],
        latest_events: &[String],
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error>;

    /// See [`get_forward_extremities_count`].
    async fn get_forward_extremities_count(&self, room_id: &str) -> Result<i64, sqlx::Error>;

    // ── context / pagination ────────────────────────────────────────────

    /// See [`find_event_id_by_timestamp`].
    async fn find_event_id_by_timestamp(
        &self,
        room_id: &str,
        ts: i64,
        forward: bool,
    ) -> Result<Option<(String, i64)>, sqlx::Error>;

    /// See [`get_events_before_context`].
    async fn get_events_before_context(
        &self,
        room_id: &str,
        before_ts: i64,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error>;

    /// See [`get_events_after_context`].
    async fn get_events_after_context(
        &self,
        room_id: &str,
        after_ts: i64,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error>;

    // ── by-type / pending / counts ──────────────────────────────────────

    /// See [`get_room_events_by_type`].
    async fn get_room_events_by_type(
        &self,
        room_id: &str,
        event_type: &str,
        limit: i64,
    ) -> Result<Vec<RoomEvent>, sqlx::Error>;

    /// See [`get_pending_room_events`].
    async fn get_pending_room_events(&self, room_id: &str, limit: i64) -> Result<Vec<RoomEvent>, sqlx::Error>;

    /// See [`get_daily_message_count`].
    async fn get_daily_message_count(&self) -> Result<i64, sqlx::Error>;

    // ── signatures / search / encryption ──────────────────────────────────

    /// See [`get_event_signatures`].
    async fn get_event_signatures(&self, event_id: &str) -> Result<Vec<EventSignature>, sqlx::Error>;

    /// See [`search_room_messages_admin`].
    async fn search_room_messages_admin(
        &self,
        room_id: &str,
        search_pattern: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error>;

    /// See [`check_room_has_encryption`].
    async fn check_room_has_encryption(&self, room_id: &str) -> Result<bool, sqlx::Error>;

    // ── incremental-sync watermarks (S14) ────────────────────────────────

    /// Returns the newest `limit` events in `room_id` whose `stream_ordering`
    /// is strictly greater than `after`, ordered ascending by stream position.
    ///
    /// Used by sliding sync incremental timelines: the client has already seen
    /// everything at or below the watermark, so only newer events are sent.
    async fn get_room_events_after_stream_ordering(
        &self,
        room_id: &str,
        after: i64,
        limit: i64,
    ) -> Result<Vec<RoomEvent>, sqlx::Error>;

    /// Returns the current maximum `stream_ordering` across all events
    /// (0 when the events table is empty). Snapshot taken at the start of a
    /// sync becomes the watermark stored on the sliding-sync token.
    async fn get_max_stream_ordering(&self) -> Result<i64, sqlx::Error>;

    /// ISSUE-03: durable txn dedup lookup — the event previously recorded
    /// for `(user_id, room_id, txn_id)`, if any.
    async fn get_event_id_by_txn(
        &self,
        user_id: &str,
        room_id: &str,
        txn_id: &str,
    ) -> Result<Option<String>, sqlx::Error>;
}

// ── EventReader delegation impl for Postgres EventStorage ───────────────

#[async_trait]
impl crate::event::reader::EventReader for super::EventStorage {
    fn pool(&self) -> &Arc<sqlx::PgPool> {
        &self.pool
    }

    async fn get_event(&self, event_id: &str) -> Result<Option<RoomEvent>, sqlx::Error> {
        self.get_event(event_id).await
    }

    async fn get_room_events(&self, room_id: &str, limit: i64) -> Result<Vec<RoomEvent>, sqlx::Error> {
        self.get_room_events(room_id, limit).await
    }

    async fn get_room_events_paginated(
        &self,
        room_id: &str,
        from: Option<i64>,
        limit: i64,
        direction: &str,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        self.get_room_events_paginated(room_id, from, limit, direction).await
    }

    async fn get_room_events_paginated_cursor(
        &self,
        room_id: &str,
        from: Option<(i64, Option<i64>)>,
        limit: i64,
        direction: &str,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        self.get_room_events_paginated_cursor(room_id, from, limit, direction).await
    }

    async fn get_room_events_batch(
        &self,
        room_ids: &[String],
        limit_per_room: i64,
    ) -> Result<HashMap<String, Vec<RoomEvent>>, sqlx::Error> {
        self.get_room_events_batch(room_ids, limit_per_room).await
    }

    async fn get_room_events_batch_since(
        &self,
        room_ids: &[String],
        since: SinceFilter,
        limit_per_room: i64,
    ) -> Result<HashMap<String, Vec<RoomEvent>>, sqlx::Error> {
        self.get_room_events_batch_since(room_ids, since, limit_per_room).await
    }

    async fn get_room_events_batch_since_filtered(
        &self,
        room_ids: &[String],
        since: SinceFilter,
        limit_per_room: i64,
        filter: &EventQueryFilter,
    ) -> Result<HashMap<String, Vec<RoomEvent>>, sqlx::Error> {
        self.get_room_events_batch_since_filtered(room_ids, since, limit_per_room, filter).await
    }

    async fn get_state_event(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> Result<Option<StateEvent>, sqlx::Error> {
        self.get_state_event(room_id, event_type, state_key).await
    }

    async fn get_state_events(&self, room_id: &str) -> Result<Vec<StateEvent>, sqlx::Error> {
        self.get_state_events(room_id).await
    }

    async fn get_state_events_by_type(&self, room_id: &str, event_type: &str) -> Result<Vec<StateEvent>, sqlx::Error> {
        self.get_state_events_by_type(room_id, event_type).await
    }

    async fn get_state_events_at_or_before(
        &self,
        room_id: &str,
        origin_server_ts: i64,
    ) -> Result<Vec<StateEvent>, sqlx::Error> {
        self.get_state_events_at_or_before(room_id, origin_server_ts).await
    }

    async fn get_events_map(&self, event_ids: &[String]) -> Result<HashMap<String, RoomEvent>, sqlx::Error> {
        self.get_events_map(event_ids).await
    }

    async fn get_max_origin_server_ts_for_room(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        self.get_max_origin_server_ts_for_room(room_id).await
    }

    async fn get_latest_event_ids_in_room(&self, room_id: &str, limit: i64) -> Result<Vec<String>, sqlx::Error> {
        self.get_latest_event_ids_in_room(room_id, limit).await
    }

    async fn count_room_events_by_status(&self, room_id: &str, status: &str) -> Result<i64, sqlx::Error> {
        self.count_room_events_by_status(room_id, status).await
    }

    async fn get_ephemeral_events(
        &self,
        room_id: &str,
        now: i64,
        limit: i64,
    ) -> Result<Vec<RoomEphemeralEvent>, sqlx::Error> {
        self.get_ephemeral_events(room_id, now, limit).await
    }

    async fn get_ephemeral_events_batch(
        &self,
        room_ids: &[String],
        now: i64,
        limit: i64,
    ) -> Result<HashMap<String, Vec<RoomEphemeralEvent>>, sqlx::Error> {
        self.get_ephemeral_events_batch(room_ids, now, limit).await
    }

    async fn get_state_events_batch(
        &self,
        room_ids: &[String],
    ) -> Result<HashMap<String, Vec<StateEvent>>, sqlx::Error> {
        self.get_state_events_batch(room_ids).await
    }

    async fn get_state_events_by_type_batch(
        &self,
        room_ids: &[String],
        event_type: &str,
    ) -> Result<HashMap<String, Vec<StateEvent>>, sqlx::Error> {
        self.get_state_events_by_type_batch(room_ids, event_type).await
    }

    async fn get_state_events_since_batch(
        &self,
        room_ids: &[String],
        since: SinceFilter,
    ) -> Result<HashMap<String, Vec<StateEvent>>, sqlx::Error> {
        self.get_state_events_since_batch(room_ids, since).await
    }

    async fn get_membership_state_keys_since_batch(
        &self,
        room_ids: &[String],
        since: SinceFilter,
    ) -> Result<HashMap<String, HashSet<String>>, sqlx::Error> {
        self.get_membership_state_keys_since_batch(room_ids, since).await
    }

    async fn get_state_change_timestamps_batch(
        &self,
        room_ids: &[String],
        since: SinceFilter,
    ) -> Result<HashMap<String, i64>, sqlx::Error> {
        self.get_state_change_timestamps_batch(room_ids, since).await
    }

    async fn get_room_events_batch_filtered(
        &self,
        room_ids: &[String],
        limit_per_room: i64,
        filter: &EventQueryFilter,
    ) -> Result<HashMap<String, Vec<RoomEvent>>, sqlx::Error> {
        self.get_room_events_batch_filtered(room_ids, limit_per_room, filter).await
    }

    async fn has_room_events_since(&self, room_ids: &[String], since: i64) -> Result<bool, sqlx::Error> {
        self.has_room_events_since(room_ids, since).await
    }

    async fn find_missing_event_ids(&self, event_ids: &[String]) -> Result<Vec<String>, sqlx::Error> {
        self.find_missing_event_ids(event_ids).await
    }

    async fn get_missing_events_between(
        &self,
        room_id: &str,
        earliest_events: &[String],
        latest_events: &[String],
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        self.get_missing_events_between(room_id, earliest_events, latest_events, limit).await
    }

    async fn get_forward_extremities_count(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        self.get_forward_extremities_count(room_id).await
    }

    async fn find_event_id_by_timestamp(
        &self,
        room_id: &str,
        ts: i64,
        forward: bool,
    ) -> Result<Option<(String, i64)>, sqlx::Error> {
        self.find_event_id_by_timestamp(room_id, ts, forward).await
    }

    async fn get_events_before_context(
        &self,
        room_id: &str,
        before_ts: i64,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        self.get_events_before_context(room_id, before_ts, limit).await
    }

    async fn get_events_after_context(
        &self,
        room_id: &str,
        after_ts: i64,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        self.get_events_after_context(room_id, after_ts, limit).await
    }

    async fn get_room_events_by_type(
        &self,
        room_id: &str,
        event_type: &str,
        limit: i64,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        self.get_room_events_by_type(room_id, event_type, limit).await
    }

    async fn get_pending_room_events(&self, room_id: &str, limit: i64) -> Result<Vec<RoomEvent>, sqlx::Error> {
        self.get_pending_room_events(room_id, limit).await
    }

    async fn get_daily_message_count(&self) -> Result<i64, sqlx::Error> {
        self.get_daily_message_count().await
    }

    async fn get_event_signatures(&self, event_id: &str) -> Result<Vec<EventSignature>, sqlx::Error> {
        self.get_event_signatures(event_id).await
    }

    async fn search_room_messages_admin(
        &self,
        room_id: &str,
        search_pattern: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        self.search_room_messages_admin(room_id, search_pattern, limit).await
    }

    async fn check_room_has_encryption(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        self.check_room_has_encryption(room_id).await
    }

    async fn get_room_events_after_stream_ordering(
        &self,
        room_id: &str,
        after: i64,
        limit: i64,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        self.get_room_events_after_stream_ordering(room_id, after, limit).await
    }

    async fn get_max_stream_ordering(&self) -> Result<i64, sqlx::Error> {
        self.get_max_stream_ordering().await
    }

    async fn get_event_id_by_txn(
        &self,
        user_id: &str,
        room_id: &str,
        txn_id: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        self.get_event_id_by_txn(user_id, room_id, txn_id).await
    }

    // ── unread counts / room state copy (moved from RoomStorage) ───────

    async fn get_unread_counts(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<crate::room::RoomUnreadCounts, sqlx::Error> {
        self.get_unread_counts(room_id, user_id).await
    }

    async fn get_unread_counts_batch(
        &self,
        room_ids: &[String],
        user_id: &str,
    ) -> Result<Vec<crate::room::RoomUnreadCounts>, sqlx::Error> {
        self.get_unread_counts_batch(room_ids, user_id).await
    }

    async fn copy_room_state(&self, source_room_id: &str, target_room_id: &str) -> Result<(), sqlx::Error> {
        self.copy_room_state(source_room_id, target_room_id).await
    }
}

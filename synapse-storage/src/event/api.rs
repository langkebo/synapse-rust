use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::models::*;

/// Storage-agnostic API for event persistence.
///
/// Implemented by [`EventStorage`] (Postgres) and [`crate::test_mocks::InMemoryEventStore`]
/// (in-memory). Services should accept `Arc<dyn EventStoreApi>` so tests can
/// swap in the in-memory backend without a database.
///
/// Follows the same seam pattern as [`crate::user::UserStore`].
#[async_trait]
pub trait EventStoreApi: Send + Sync {
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

    async fn get_room_events_since_batch(
        &self,
        room_ids: &[String],
        since: i64,
        limit_per_room: i64,
    ) -> Result<HashMap<String, Vec<RoomEvent>>, sqlx::Error>;

    async fn get_room_events_since_stream_batch(
        &self,
        room_ids: &[String],
        since_stream_ordering: i64,
        limit_per_room: i64,
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

    // ── mutation ──────────────────────────────────────────────────────

    async fn create_event(
        &self,
        params: CreateEventParams,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error>;

    async fn update_event_signatures_and_hashes(
        &self,
        event_id: &str,
        signatures: &serde_json::Value,
        hashes: &serde_json::Value,
    ) -> Result<(), sqlx::Error>;

    async fn redact_event_content(&self, event_id: &str, redacted_by: Option<&str>) -> Result<(), sqlx::Error>;

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

    async fn get_room_events_since_batch_filtered(
        &self,
        room_ids: &[String],
        since: i64,
        limit_per_room: i64,
        filter: &EventQueryFilter,
    ) -> Result<HashMap<String, Vec<RoomEvent>>, sqlx::Error>;

    async fn get_room_events_since_stream_batch_filtered(
        &self,
        room_ids: &[String],
        since_stream_ordering: i64,
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

    // ── mutation: graph / signatures / reports ─────────────────────────

    #[allow(clippy::too_many_arguments)]
    async fn create_event_with_graph(
        &self,
        params: CreateEventParams,
        prev_events: &[String],
        auth_events: &[String],
        depth: i64,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error>;

    #[allow(clippy::too_many_arguments)]
    async fn save_event_signature(
        &self,
        event_id: &str,
        user_id: &str,
        device_id: &str,
        signature: &str,
        key_id: &str,
        algorithm: &str,
        created_ts: i64,
    ) -> Result<(), sqlx::Error>;

    async fn get_event_signatures(&self, event_id: &str) -> Result<Vec<EventSignature>, sqlx::Error>;

    async fn report_event(
        &self,
        event_id: &str,
        room_id: &str,
        reported_user_id: &str,
        reporter_user_id: &str,
        reason: Option<&str>,
        score: i32,
    ) -> Result<i64, sqlx::Error>;

    async fn search_room_messages_admin(
        &self,
        room_id: &str,
        search_pattern: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error>;

    // ── ephemeral mutations ─────────────────────────────────────────────

    async fn add_ephemeral_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
        content: &serde_json::Value,
        stream_id: i64,
    ) -> Result<(), sqlx::Error>;

    #[allow(clippy::too_many_arguments)]
    async fn upsert_ephemeral_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
        content: &serde_json::Value,
        stream_id: i64,
        created_ts: i64,
        expires_at: Option<i64>,
    ) -> Result<(), sqlx::Error>;

    async fn delete_ephemeral_event(&self, room_id: &str, event_type: &str, user_id: &str) -> Result<(), sqlx::Error>;

    // ── encryption / retention ─────────────────────────────────────────────

    async fn check_room_has_encryption(&self, room_id: &str) -> Result<bool, sqlx::Error>;

    async fn delete_events_before(&self, room_id: &str, timestamp: i64) -> Result<u64, sqlx::Error>;

    async fn upsert_power_levels_event(
        &self,
        event_id: &str,
        room_id: &str,
        user_id: &str,
        content: serde_json::Value,
        origin_server_ts: i64,
        sender: &str,
    ) -> Result<(), sqlx::Error>;
}

// ── Delegation impl for the Postgres EventStorage ────────────────────

#[async_trait]
impl EventStoreApi for super::EventStorage {
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

    async fn get_room_events_batch(
        &self,
        room_ids: &[String],
        limit_per_room: i64,
    ) -> Result<HashMap<String, Vec<RoomEvent>>, sqlx::Error> {
        self.get_room_events_batch(room_ids, limit_per_room).await
    }

    async fn get_room_events_since_batch(
        &self,
        room_ids: &[String],
        since: i64,
        limit_per_room: i64,
    ) -> Result<HashMap<String, Vec<RoomEvent>>, sqlx::Error> {
        self.get_room_events_since_batch(room_ids, since, limit_per_room).await
    }

    async fn get_room_events_since_stream_batch(
        &self,
        room_ids: &[String],
        since_stream_ordering: i64,
        limit_per_room: i64,
    ) -> Result<HashMap<String, Vec<RoomEvent>>, sqlx::Error> {
        self.get_room_events_since_stream_batch(room_ids, since_stream_ordering, limit_per_room).await
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

    async fn create_event(
        &self,
        params: CreateEventParams,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error> {
        self.create_event(params, tx).await
    }

    async fn update_event_signatures_and_hashes(
        &self,
        event_id: &str,
        signatures: &serde_json::Value,
        hashes: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        self.update_event_signatures_and_hashes(event_id, signatures, hashes).await
    }

    async fn redact_event_content(&self, event_id: &str, redacted_by: Option<&str>) -> Result<(), sqlx::Error> {
        self.redact_event_content(event_id, redacted_by).await
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

    async fn get_room_events_since_batch_filtered(
        &self,
        room_ids: &[String],
        since: i64,
        limit_per_room: i64,
        filter: &EventQueryFilter,
    ) -> Result<HashMap<String, Vec<RoomEvent>>, sqlx::Error> {
        self.get_room_events_since_batch_filtered(room_ids, since, limit_per_room, filter).await
    }

    async fn get_room_events_since_stream_batch_filtered(
        &self,
        room_ids: &[String],
        since_stream_ordering: i64,
        limit_per_room: i64,
        filter: &EventQueryFilter,
    ) -> Result<HashMap<String, Vec<RoomEvent>>, sqlx::Error> {
        self.get_room_events_since_stream_batch_filtered(room_ids, since_stream_ordering, limit_per_room, filter).await
    }

    async fn has_room_events_since(&self, room_ids: &[String], since: i64) -> Result<bool, sqlx::Error> {
        self.has_room_events_since(room_ids, since).await
    }

    // ── graph / dag ──────────────────────────────────────────────────────

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

    // ── context / pagination ────────────────────────────────────────────

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

    // ── by-type / pending / counts ──────────────────────────────────────

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

    // ── mutation: graph / signatures / reports ─────────────────────────

    async fn create_event_with_graph(
        &self,
        params: CreateEventParams,
        prev_events: &[String],
        auth_events: &[String],
        depth: i64,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error> {
        self.create_event_with_graph(params, prev_events, auth_events, depth, tx).await
    }

    async fn save_event_signature(
        &self,
        event_id: &str,
        user_id: &str,
        device_id: &str,
        signature: &str,
        key_id: &str,
        algorithm: &str,
        created_ts: i64,
    ) -> Result<(), sqlx::Error> {
        self.save_event_signature(event_id, user_id, device_id, signature, key_id, algorithm, created_ts).await
    }

    async fn get_event_signatures(&self, event_id: &str) -> Result<Vec<EventSignature>, sqlx::Error> {
        self.get_event_signatures(event_id).await
    }

    async fn report_event(
        &self,
        event_id: &str,
        room_id: &str,
        reported_user_id: &str,
        reporter_user_id: &str,
        reason: Option<&str>,
        score: i32,
    ) -> Result<i64, sqlx::Error> {
        self.report_event(event_id, room_id, reported_user_id, reporter_user_id, reason, score).await
    }

    async fn search_room_messages_admin(
        &self,
        room_id: &str,
        search_pattern: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        self.search_room_messages_admin(room_id, search_pattern, limit).await
    }

    // ── ephemeral mutations ─────────────────────────────────────────────

    async fn add_ephemeral_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
        content: &serde_json::Value,
        stream_id: i64,
    ) -> Result<(), sqlx::Error> {
        self.add_ephemeral_event(room_id, user_id, event_type, content, stream_id).await
    }

    async fn upsert_ephemeral_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
        content: &serde_json::Value,
        stream_id: i64,
        created_ts: i64,
        expires_at: Option<i64>,
    ) -> Result<(), sqlx::Error> {
        self.upsert_ephemeral_event(room_id, user_id, event_type, content, stream_id, created_ts, expires_at).await
    }

    async fn delete_ephemeral_event(&self, room_id: &str, event_type: &str, user_id: &str) -> Result<(), sqlx::Error> {
        self.delete_ephemeral_event(room_id, event_type, user_id).await
    }

    // ── encryption / retention ─────────────────────────────────────────────

    async fn check_room_has_encryption(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        self.check_room_has_encryption(room_id).await
    }

    async fn delete_events_before(&self, room_id: &str, timestamp: i64) -> Result<u64, sqlx::Error> {
        self.delete_events_before(room_id, timestamp).await
    }

    async fn upsert_power_levels_event(
        &self,
        event_id: &str,
        room_id: &str,
        user_id: &str,
        content: serde_json::Value,
        origin_server_ts: i64,
        sender: &str,
    ) -> Result<(), sqlx::Error> {
        self.upsert_power_levels_event(event_id, room_id, user_id, content, origin_server_ts, sender).await
    }
}

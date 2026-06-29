use async_trait::async_trait;
use std::sync::Arc;

use super::models::{CreateEventParams, EventQueryFilter, RoomEphemeralEvent, RoomEvent, StateEvent};

#[async_trait]
pub trait EventRepository: Send + Sync {
    /// Returns a reference to the database connection pool.
    fn pool(&self) -> &Arc<sqlx::PgPool>;

    async fn get_event(&self, event_id: &str) -> Result<Option<RoomEvent>, sqlx::Error>;

    async fn create_event(
        &self,
        params: CreateEventParams,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error>;

    async fn create_event_with_graph(
        &self,
        params: CreateEventParams,
        prev_events: &[String],
        auth_events: &[String],
        depth: i64,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error>;

    /// Retrieve paginated room events.
    async fn get_room_events_paginated(
        &self,
        room_id: &str,
        from: Option<i64>,
        limit: i64,
        direction: &str,
    ) -> Result<Vec<RoomEvent>, sqlx::Error>;

    async fn get_events_batch(&self, event_ids: &[String]) -> Result<Vec<RoomEvent>, sqlx::Error>;

    // -- room event batch queries (sync-oriented) --

    async fn get_room_events_batch(
        &self,
        room_ids: &[String],
        limit_per_room: i64,
    ) -> Result<std::collections::HashMap<String, Vec<RoomEvent>>, sqlx::Error>;

    async fn get_room_events_batch_filtered(
        &self,
        room_ids: &[String],
        limit_per_room: i64,
        filter: &EventQueryFilter,
    ) -> Result<std::collections::HashMap<String, Vec<RoomEvent>>, sqlx::Error>;

    async fn get_room_events_since_batch(
        &self,
        room_ids: &[String],
        since: i64,
        limit_per_room: i64,
    ) -> Result<std::collections::HashMap<String, Vec<RoomEvent>>, sqlx::Error>;

    async fn get_room_events_since_stream_batch(
        &self,
        room_ids: &[String],
        since_stream_ordering: i64,
        limit_per_room: i64,
    ) -> Result<std::collections::HashMap<String, Vec<RoomEvent>>, sqlx::Error>;

    async fn get_room_events_since_batch_filtered(
        &self,
        room_ids: &[String],
        since: i64,
        limit_per_room: i64,
        filter: &EventQueryFilter,
    ) -> Result<std::collections::HashMap<String, Vec<RoomEvent>>, sqlx::Error>;

    async fn get_room_events_since_stream_batch_filtered(
        &self,
        room_ids: &[String],
        since_stream_ordering: i64,
        limit_per_room: i64,
        filter: &EventQueryFilter,
    ) -> Result<std::collections::HashMap<String, Vec<RoomEvent>>, sqlx::Error>;

    async fn has_room_events_since(&self, room_ids: &[String], since: i64) -> Result<bool, sqlx::Error>;

    async fn get_state_event(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> Result<Option<StateEvent>, sqlx::Error>;

    async fn get_state_events(&self, room_id: &str) -> Result<Vec<StateEvent>, sqlx::Error>;

    async fn get_state_events_batch(
        &self,
        room_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<StateEvent>>, sqlx::Error>;

    async fn get_state_events_by_type_batch(
        &self,
        room_ids: &[String],
        event_type: &str,
    ) -> Result<std::collections::HashMap<String, Vec<StateEvent>>, sqlx::Error>;

    async fn get_state_events_since_batch(
        &self,
        room_ids: &[String],
        since: i64,
    ) -> Result<std::collections::HashMap<String, Vec<StateEvent>>, sqlx::Error>;

    async fn get_state_events_since_stream_batch(
        &self,
        room_ids: &[String],
        since_stream_ordering: i64,
    ) -> Result<std::collections::HashMap<String, Vec<StateEvent>>, sqlx::Error>;

    async fn get_state_change_timestamps_batch(
        &self,
        room_ids: &[String],
        since: i64,
    ) -> Result<std::collections::HashMap<String, i64>, sqlx::Error>;

    async fn get_state_change_timestamps_since_stream_batch(
        &self,
        room_ids: &[String],
        since_stream_ordering: i64,
    ) -> Result<std::collections::HashMap<String, i64>, sqlx::Error>;

    async fn get_membership_state_keys_since_batch(
        &self,
        room_ids: &[String],
        since: i64,
    ) -> Result<std::collections::HashMap<String, std::collections::HashSet<String>>, sqlx::Error>;

    async fn get_membership_state_keys_since_stream_batch(
        &self,
        room_ids: &[String],
        since_stream_ordering: i64,
    ) -> Result<std::collections::HashMap<String, std::collections::HashSet<String>>, sqlx::Error>;

    /// Retrieve paginated room events (string-based cursor variant).
    ///
    /// NOTE: `to` and `filter` parameters are not yet supported by the
    /// Postgres implementation.  Non-None values will produce a
    /// `tracing::warn!` log at runtime but are otherwise silently ignored.
    async fn get_room_events_paginated_with_filter(
        &self,
        room_id: &str,
        from: Option<&str>,
        to: Option<&str>,
        limit: i64,
        filter: Option<&EventQueryFilter>,
    ) -> Result<Vec<RoomEvent>, sqlx::Error>;

    async fn get_room_create_event(&self, room_id: &str) -> Result<Option<RoomEvent>, sqlx::Error>;

    async fn count_room_events(&self, room_id: &str) -> Result<i64, sqlx::Error>;

    async fn search_postgres_messages(
        &self,
        room_id: &str,
        search_term: &str,
        limit: i64,
    ) -> Result<Vec<RoomEvent>, sqlx::Error>;

    // -- ephemeral events --

    async fn add_ephemeral_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
        content: &serde_json::Value,
        stream_id: i64,
    ) -> Result<(), sqlx::Error>;

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
    ) -> Result<std::collections::HashMap<String, Vec<RoomEphemeralEvent>>, sqlx::Error>;

    // -- event DAG helpers --

    async fn find_event_id_by_timestamp(
        &self,
        room_id: &str,
        ts: i64,
        forward: bool,
    ) -> Result<Option<(String, i64)>, sqlx::Error>;

    async fn get_room_events(&self, room_id: &str, limit: i64) -> Result<Vec<RoomEvent>, sqlx::Error>;

    async fn get_room_events_by_type(
        &self,
        room_id: &str,
        event_type: &str,
        limit: i64,
    ) -> Result<Vec<RoomEvent>, sqlx::Error>;

    // -- event reporting --

    async fn report_event(
        &self,
        event_id: &str,
        room_id: &str,
        _reported_user_id: &str,
        reporter_user_id: &str,
        reason: Option<&str>,
        score: i32,
    ) -> Result<i64, sqlx::Error>;

    // -- redaction --

    async fn redact_event_content(&self, event_id: &str, redacted_by: Option<&str>) -> Result<(), sqlx::Error>;

    // -- event signatures --

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

    async fn get_event_signatures(&self, event_id: &str) -> Result<Vec<super::models::EventSignature>, sqlx::Error>;

    // -- power levels --

    async fn upsert_power_levels_event(
        &self,
        event_id: &str,
        room_id: &str,
        user_id: &str,
        content: serde_json::Value,
        origin_server_ts: i64,
        sender: &str,
    ) -> Result<(), sqlx::Error>;

    // -- context --

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

    // -- search --

    async fn search_room_messages_admin(
        &self,
        room_id: &str,
        search_pattern: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error>;

    // -- DAG / forward extremities --

    async fn get_forward_extremities_count(&self, room_id: &str) -> Result<i64, sqlx::Error>;

    async fn get_latest_event_ids_in_room(&self, room_id: &str, limit: i64) -> Result<Vec<String>, sqlx::Error>;

    // -- missing events / DAG gap fill --

    async fn get_missing_events_between(
        &self,
        room_id: &str,
        earliest_events: &[String],
        latest_events: &[String],
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error>;

    // -- signature/hash update --

    async fn update_event_signatures_and_hashes(
        &self,
        event_id: &str,
        signatures: &serde_json::Value,
        hashes: &serde_json::Value,
    ) -> Result<(), sqlx::Error>;

    // -- missing event IDs --

    async fn find_missing_event_ids(&self, event_ids: &[String]) -> Result<Vec<String>, sqlx::Error>;

    // -- state events (filtered) --

    async fn get_state_events_at_or_before(
        &self,
        room_id: &str,
        origin_server_ts: i64,
    ) -> Result<Vec<StateEvent>, sqlx::Error>;

    async fn get_state_events_by_type(&self, room_id: &str, event_type: &str) -> Result<Vec<StateEvent>, sqlx::Error>;

    // -- cleanup --

    async fn delete_events_before(&self, room_id: &str, timestamp: i64) -> Result<u64, sqlx::Error>;

    // -- batch helpers --

    async fn get_max_origin_server_ts_for_room(&self, room_id: &str) -> Result<i64, sqlx::Error>;

    async fn check_room_has_encryption(&self, room_id: &str) -> Result<bool, sqlx::Error>;

    // -- pending events --

    async fn get_pending_room_events(&self, room_id: &str, limit: i64) -> Result<Vec<RoomEvent>, sqlx::Error>;

    // -- counting --

    async fn count_room_events_by_status(&self, room_id: &str, status: &str) -> Result<i64, sqlx::Error>;

    // -- daily message count --

    async fn get_daily_message_count(&self) -> Result<i64, sqlx::Error>;
}

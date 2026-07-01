use async_trait::async_trait;
use sqlx::{Pool, Postgres};
use std::sync::Arc;

use super::repository::EventRepository;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RoomEvent {
    pub event_id: String,
    pub room_id: String,
    pub user_id: String,
    pub event_type: String,
    pub content: serde_json::Value,
    pub state_key: Option<String>,
    pub depth: i64,
    pub origin_server_ts: i64,
    #[sqlx(rename = "processed_at")]
    pub processed_ts: i64,
    pub not_before: i64,
    pub status: Option<String>,
    pub reference_image: Option<String>,
    pub origin: String,
    pub stream_ordering: Option<i64>,
    /// Target event_id for `m.room.redaction` events (P0-05).  `None` for
    /// non-redaction events or redaction events that do not specify a target.
    pub redacts: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StateEvent {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub event_type: Option<String>,
    pub content: serde_json::Value,
    pub state_key: Option<String>,
    pub unsigned: Option<serde_json::Value>,
    pub is_redacted: Option<bool>,
    pub origin_server_ts: i64,
    pub depth: Option<i64>,
    #[sqlx(rename = "processed_at")]
    pub processed_ts: Option<i64>,
    pub not_before: Option<i64>,
    pub status: Option<String>,
    pub reference_image: Option<String>,
    pub origin: Option<String>,
    pub user_id: Option<String>,
    pub stream_ordering: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RoomEphemeralEvent {
    pub event_type: String,
    pub user_id: String,
    pub content: serde_json::Value,
    pub stream_id: i64,
    pub created_ts: i64,
}

#[derive(Clone)]
pub struct EventStorage {
    pub pool: Arc<Pool<Postgres>>,
    pub server_name: String,
}

#[derive(Debug, Clone)]
pub struct CreateEventParams {
    pub event_id: String,
    pub room_id: String,
    pub user_id: String,
    pub event_type: String,
    pub content: serde_json::Value,
    pub state_key: Option<String>,
    pub origin_server_ts: i64,
    /// Target event_id for `m.room.redaction` events (P0-05).  Set to `None`
    /// for non-redaction events.  For v1-v10 this is populated from the
    /// top-level `redacts` PDU field; for v11+ from `content.redacts`.
    pub redacts: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EventQueryFilter {
    pub types: Option<Vec<String>>,
    pub not_types: Option<Vec<String>>,
    pub senders: Option<Vec<String>>,
    pub not_senders: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// Event signature model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EventSignature {
    pub id: uuid::Uuid,
    pub event_id: String,
    pub user_id: String,
    pub device_id: String,
    pub signature: String,
    pub key_id: String,
    pub created_ts: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EventReportId {
    pub id: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EventReport {
    pub id: i64,
    pub event_id: String,
    pub room_id: String,
    pub reporter_user_id: String,
    pub reason: Option<String>,
    pub score: i32,
    pub received_ts: i64,
    #[sqlx(rename = "resolved_at")]
    pub resolved_ts: Option<i64>,
    pub resolved_by: Option<String>,
}

#[async_trait]
impl EventRepository for EventStorage {
    fn pool(&self) -> &Arc<Pool<Postgres>> {
        &self.pool
    }

    async fn get_event(&self, event_id: &str) -> Result<Option<RoomEvent>, sqlx::Error> {
        self.get_event(event_id).await
    }

    async fn create_event(
        &self,
        params: CreateEventParams,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error> {
        EventStorage::create_event(self, params, tx).await
    }

    async fn create_event_with_graph(
        &self,
        params: CreateEventParams,
        prev_events: &[String],
        auth_events: &[String],
        depth: i64,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error> {
        EventStorage::create_event_with_graph(self, params, prev_events, auth_events, depth, tx).await
    }

    async fn get_room_events_paginated(
        &self,
        room_id: &str,
        from: Option<i64>,
        limit: i64,
        direction: &str,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        EventStorage::get_room_events_paginated(self, room_id, from, limit, direction).await
    }

    async fn get_events_batch(&self, event_ids: &[String]) -> Result<Vec<RoomEvent>, sqlx::Error> {
        self.get_events_batch(event_ids).await
    }

    // -- room event batch queries (sync-oriented) --

    async fn get_room_events_batch(
        &self,
        room_ids: &[String],
        limit_per_room: i64,
    ) -> Result<std::collections::HashMap<String, Vec<RoomEvent>>, sqlx::Error> {
        self.get_room_events_batch(room_ids, limit_per_room).await
    }

    async fn get_room_events_batch_filtered(
        &self,
        room_ids: &[String],
        limit_per_room: i64,
        filter: &EventQueryFilter,
    ) -> Result<std::collections::HashMap<String, Vec<RoomEvent>>, sqlx::Error> {
        self.get_room_events_batch_filtered(room_ids, limit_per_room, filter).await
    }

    async fn get_room_events_since_batch(
        &self,
        room_ids: &[String],
        since: i64,
        limit_per_room: i64,
    ) -> Result<std::collections::HashMap<String, Vec<RoomEvent>>, sqlx::Error> {
        self.get_room_events_since_batch(room_ids, since, limit_per_room).await
    }

    async fn get_room_events_since_stream_batch(
        &self,
        room_ids: &[String],
        since_stream_ordering: i64,
        limit_per_room: i64,
    ) -> Result<std::collections::HashMap<String, Vec<RoomEvent>>, sqlx::Error> {
        self.get_room_events_since_stream_batch(room_ids, since_stream_ordering, limit_per_room).await
    }

    async fn get_room_events_since_batch_filtered(
        &self,
        room_ids: &[String],
        since: i64,
        limit_per_room: i64,
        filter: &EventQueryFilter,
    ) -> Result<std::collections::HashMap<String, Vec<RoomEvent>>, sqlx::Error> {
        self.get_room_events_since_batch_filtered(room_ids, since, limit_per_room, filter).await
    }

    async fn get_room_events_since_stream_batch_filtered(
        &self,
        room_ids: &[String],
        since_stream_ordering: i64,
        limit_per_room: i64,
        filter: &EventQueryFilter,
    ) -> Result<std::collections::HashMap<String, Vec<RoomEvent>>, sqlx::Error> {
        self.get_room_events_since_stream_batch_filtered(room_ids, since_stream_ordering, limit_per_room, filter).await
    }

    async fn has_room_events_since(&self, room_ids: &[String], since: i64) -> Result<bool, sqlx::Error> {
        self.has_room_events_since(room_ids, since).await
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

    async fn get_state_events_batch(
        &self,
        room_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<StateEvent>>, sqlx::Error> {
        self.get_state_events_batch(room_ids).await
    }

    async fn get_state_events_by_type_batch(
        &self,
        room_ids: &[String],
        event_type: &str,
    ) -> Result<std::collections::HashMap<String, Vec<StateEvent>>, sqlx::Error> {
        self.get_state_events_by_type_batch(room_ids, event_type).await
    }

    async fn get_state_events_since_batch(
        &self,
        room_ids: &[String],
        since: i64,
    ) -> Result<std::collections::HashMap<String, Vec<StateEvent>>, sqlx::Error> {
        self.get_state_events_since_batch(room_ids, since).await
    }

    async fn get_state_events_since_stream_batch(
        &self,
        room_ids: &[String],
        since_stream_ordering: i64,
    ) -> Result<std::collections::HashMap<String, Vec<StateEvent>>, sqlx::Error> {
        self.get_state_events_since_stream_batch(room_ids, since_stream_ordering).await
    }

    async fn get_state_change_timestamps_batch(
        &self,
        room_ids: &[String],
        since: i64,
    ) -> Result<std::collections::HashMap<String, i64>, sqlx::Error> {
        self.get_state_change_timestamps_batch(room_ids, since).await
    }

    async fn get_state_change_timestamps_since_stream_batch(
        &self,
        room_ids: &[String],
        since_stream_ordering: i64,
    ) -> Result<std::collections::HashMap<String, i64>, sqlx::Error> {
        self.get_state_change_timestamps_since_stream_batch(room_ids, since_stream_ordering).await
    }

    async fn get_membership_state_keys_since_batch(
        &self,
        room_ids: &[String],
        since: i64,
    ) -> Result<std::collections::HashMap<String, std::collections::HashSet<String>>, sqlx::Error> {
        self.get_membership_state_keys_since_batch(room_ids, since).await
    }

    async fn get_membership_state_keys_since_stream_batch(
        &self,
        room_ids: &[String],
        since_stream_ordering: i64,
    ) -> Result<std::collections::HashMap<String, std::collections::HashSet<String>>, sqlx::Error> {
        self.get_membership_state_keys_since_stream_batch(room_ids, since_stream_ordering).await
    }

    async fn get_room_events_paginated_with_filter(
        &self,
        room_id: &str,
        from: Option<&str>,
        to: Option<&str>,
        limit: i64,
        filter: Option<&EventQueryFilter>,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        // Warnings for unsupported to/filter are in the inherent method
        self.get_room_events_paginated_with_filter(room_id, from, to, limit, filter).await
    }

    async fn get_room_create_event(&self, room_id: &str) -> Result<Option<RoomEvent>, sqlx::Error> {
        self.get_room_create_event(room_id).await
    }

    async fn count_room_events(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        self.count_room_events(room_id).await
    }

    async fn search_postgres_messages(
        &self,
        room_id: &str,
        search_term: &str,
        limit: i64,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        self.search_room_postgres_messages(room_id, search_term, limit).await
    }

    // -- ephemeral events --

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
    ) -> Result<std::collections::HashMap<String, Vec<RoomEphemeralEvent>>, sqlx::Error> {
        self.get_ephemeral_events_batch(room_ids, now, limit).await
    }

    // -- event DAG helpers --

    async fn find_event_id_by_timestamp(
        &self,
        room_id: &str,
        ts: i64,
        forward: bool,
    ) -> Result<Option<(String, i64)>, sqlx::Error> {
        self.find_event_id_by_timestamp(room_id, ts, forward).await
    }

    async fn get_room_events(&self, room_id: &str, limit: i64) -> Result<Vec<RoomEvent>, sqlx::Error> {
        self.get_room_events(room_id, limit).await
    }

    async fn get_room_events_by_type(
        &self,
        room_id: &str,
        event_type: &str,
        limit: i64,
    ) -> Result<Vec<RoomEvent>, sqlx::Error> {
        self.get_room_events_by_type(room_id, event_type, limit).await
    }

    // -- event reporting --

    async fn report_event(
        &self,
        event_id: &str,
        room_id: &str,
        _reported_user_id: &str,
        reporter_user_id: &str,
        reason: Option<&str>,
        score: i32,
    ) -> Result<i64, sqlx::Error> {
        self.report_event(event_id, room_id, _reported_user_id, reporter_user_id, reason, score).await
    }

    // -- redaction --

    async fn redact_event_content(&self, event_id: &str, redacted_by: Option<&str>) -> Result<(), sqlx::Error> {
        self.redact_event_content(event_id, redacted_by).await
    }

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
    ) -> Result<(), sqlx::Error> {
        self.save_event_signature(event_id, user_id, device_id, signature, key_id, algorithm, created_ts).await
    }

    async fn get_event_signatures(&self, event_id: &str) -> Result<Vec<EventSignature>, sqlx::Error> {
        self.get_event_signatures(event_id).await
    }

    // -- power levels --

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

    // -- context --

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

    // -- search --

    async fn search_room_messages_admin(
        &self,
        room_id: &str,
        search_pattern: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        self.search_room_messages_admin(room_id, search_pattern, limit).await
    }

    // -- DAG / forward extremities --

    async fn get_forward_extremities_count(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        self.get_forward_extremities_count(room_id).await
    }

    async fn get_latest_event_ids_in_room(&self, room_id: &str, limit: i64) -> Result<Vec<String>, sqlx::Error> {
        self.get_latest_event_ids_in_room(room_id, limit).await
    }

    // -- missing events / DAG gap fill --

    async fn get_missing_events_between(
        &self,
        room_id: &str,
        earliest_events: &[String],
        latest_events: &[String],
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        self.get_missing_events_between(room_id, earliest_events, latest_events, limit).await
    }

    // -- signature/hash update --

    async fn update_event_signatures_and_hashes(
        &self,
        event_id: &str,
        signatures: &serde_json::Value,
        hashes: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        self.update_event_signatures_and_hashes(event_id, signatures, hashes).await
    }

    // -- missing event IDs --

    async fn find_missing_event_ids(&self, event_ids: &[String]) -> Result<Vec<String>, sqlx::Error> {
        self.find_missing_event_ids(event_ids).await
    }

    // -- state events (filtered) --

    async fn get_state_events_at_or_before(
        &self,
        room_id: &str,
        origin_server_ts: i64,
    ) -> Result<Vec<StateEvent>, sqlx::Error> {
        self.get_state_events_at_or_before(room_id, origin_server_ts).await
    }

    async fn get_state_events_by_type(&self, room_id: &str, event_type: &str) -> Result<Vec<StateEvent>, sqlx::Error> {
        self.get_state_events_by_type(room_id, event_type).await
    }

    // -- cleanup --

    async fn delete_events_before(&self, room_id: &str, timestamp: i64) -> Result<u64, sqlx::Error> {
        self.delete_events_before(room_id, timestamp).await
    }

    // -- batch helpers --

    async fn get_max_origin_server_ts_for_room(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        self.get_max_origin_server_ts_for_room(room_id).await
    }

    async fn check_room_has_encryption(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        self.check_room_has_encryption(room_id).await
    }

    // -- pending events --

    async fn get_pending_room_events(&self, room_id: &str, limit: i64) -> Result<Vec<RoomEvent>, sqlx::Error> {
        self.get_pending_room_events(room_id, limit).await
    }

    // -- counting --

    async fn count_room_events_by_status(&self, room_id: &str, status: &str) -> Result<i64, sqlx::Error> {
        self.count_room_events_by_status(room_id, status).await
    }

    // -- daily message count --

    async fn get_daily_message_count(&self) -> Result<i64, sqlx::Error> {
        self.get_daily_message_count().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now_ms() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

    #[test]
    fn test_room_event_construction() {
        let ev = RoomEvent {
            event_id: "$ev1:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({"body": "hello"}),
            state_key: None,
            depth: 1,
            origin_server_ts: now_ms(),
            processed_ts: now_ms(),
            not_before: 0,
            status: None,
            reference_image: None,
            origin: "example.com".to_string(),
            stream_ordering: Some(1),
            redacts: None,
        };
        assert_eq!(ev.event_id, "$ev1:example.com");
        assert_eq!(ev.event_type, "m.room.message");
        assert!(ev.state_key.is_none());
        assert!(ev.redacts.is_none());
        assert_eq!(ev.origin, "example.com");
    }

    #[test]
    fn test_room_event_redaction_field() {
        let ev = RoomEvent {
            event_id: "$redact:example.com".to_string(),
            room_id: "!r:example.com".to_string(),
            user_id: "@admin:example.com".to_string(),
            event_type: "m.room.redaction".to_string(),
            content: serde_json::json!({}),
            state_key: None,
            depth: 2,
            origin_server_ts: now_ms(),
            processed_ts: now_ms(),
            not_before: 0,
            status: None,
            reference_image: None,
            origin: "example.com".to_string(),
            stream_ordering: None,
            redacts: Some("$target:example.com".to_string()),
        };
        assert_eq!(ev.redacts.as_deref(), Some("$target:example.com"));
    }

    #[test]
    fn test_state_event_construction_optional_fields() {
        let se = StateEvent {
            event_id: "$se1:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            sender: "@bob:example.com".to_string(),
            event_type: Some("m.room.name".to_string()),
            content: serde_json::json!({"name": "Test Room"}),
            state_key: Some("".to_string()),
            unsigned: None,
            is_redacted: Some(false),
            origin_server_ts: now_ms(),
            depth: Some(1),
            processed_ts: None,
            not_before: None,
            status: None,
            reference_image: None,
            origin: None,
            user_id: None,
            stream_ordering: None,
        };
        assert_eq!(se.event_type.as_deref(), Some("m.room.name"));
        assert_eq!(se.state_key.as_deref(), Some(""));
        assert_eq!(se.is_redacted, Some(false));
    }

    #[test]
    fn test_room_ephemeral_event_construction() {
        let ee = RoomEphemeralEvent {
            event_type: "m.typing".to_string(),
            user_id: "@alice:example.com".to_string(),
            content: serde_json::json!({"user_ids": []}),
            stream_id: 42,
            created_ts: now_ms(),
        };
        assert_eq!(ee.event_type, "m.typing");
        assert_eq!(ee.stream_id, 42);
        assert!(ee.created_ts > 0);
    }

    #[test]
    fn test_create_event_params_construction() {
        let params = CreateEventParams {
            event_id: "$new:example.com".to_string(),
            room_id: "!r:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({"body": "hi"}),
            state_key: None,
            origin_server_ts: now_ms(),
            redacts: None,
        };
        assert_eq!(params.event_type, "m.room.message");
        assert!(params.state_key.is_none());
        assert!(params.redacts.is_none());
    }

    #[test]
    fn test_event_query_filter_default_all_none() {
        let filter = EventQueryFilter::default();
        assert!(filter.types.is_none());
        assert!(filter.not_types.is_none());
        assert!(filter.senders.is_none());
        assert!(filter.not_senders.is_none());
    }

    #[test]
    fn test_event_query_filter_equality() {
        let f1 = EventQueryFilter {
            types: Some(vec!["m.room.message".to_string()]),
            not_types: None,
            senders: Some(vec!["@alice:example.com".to_string()]),
            not_senders: None,
        };
        let f2 = f1.clone();
        assert_eq!(f1, f2);
    }

    #[test]
    fn test_event_query_filter_inequality() {
        let f1 = EventQueryFilter {
            types: Some(vec!["m.room.message".to_string()]),
            not_types: None,
            senders: None,
            not_senders: None,
        };
        let f2 = EventQueryFilter {
            types: Some(vec!["m.room.member".to_string()]),
            not_types: None,
            senders: None,
            not_senders: None,
        };
        assert_ne!(f1, f2);
    }

    #[test]
    fn test_event_signature_construction() {
        let sig = EventSignature {
            id: uuid::Uuid::new_v4(),
            event_id: "$ev:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            device_id: "DEV1".to_string(),
            signature: "sig_base64".to_string(),
            key_id: "ed25519:1".to_string(),
            created_ts: Some(now_ms()),
        };
        assert_eq!(sig.device_id, "DEV1");
        assert_eq!(sig.key_id, "ed25519:1");
        assert!(sig.created_ts.is_some());
    }

    #[test]
    fn test_event_report_construction() {
        let report = EventReport {
            id: 1,
            event_id: "$bad:example.com".to_string(),
            room_id: "!r:example.com".to_string(),
            reporter_user_id: "@bob:example.com".to_string(),
            reason: Some("spam".to_string()),
            score: 50,
            received_ts: now_ms(),
            resolved_ts: None,
            resolved_by: None,
        };
        assert_eq!(report.score, 50);
        assert_eq!(report.reason.as_deref(), Some("spam"));
        assert!(report.resolved_ts.is_none());
    }
}

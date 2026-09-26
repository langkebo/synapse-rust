//! Graph-metadata resolution and the write-path decoration that persists it.
//!
//! # The defect this closes
//!
//! Locally-produced events were written by `EventStorage::create_event`, whose
//! INSERT has no `depth` / `prev_events` / `auth_events` columns. The federation
//! PDU projector (`synapse-web/src/routes/federation/pdu.rs`) classifies such a
//! row as `MissingGraphMetadata` and **refuses to sign it** — a peer joining one
//! of our rooms received unsigned state it could not verify, and our outbound
//! `/send` PDUs lacked two mandatory keys.
//!
//! # Where this runs
//!
//! Every service persists room events through `Arc<dyn EventWriter>`
//! (see `notifying_event_writer`'s module docs), and that trait object is
//! assembled in exactly one place (`wiring/rooms.rs`). Decorating it there
//! covers messaging, membership, moderation, lifecycle and backfill without
//! touching a single call site.
//!
//! # Transactions
//!
//! Resolution reads the *committed* room state and extremities through
//! [`GraphMetadataSource`]. Inside a caller-managed transaction those reads
//! cannot see the caller's uncommitted rows, so resolving there would silently
//! produce **wrong** graph data (an incomplete `auth_events`, or
//! `prev_events: []` for an event that is not the DAG root) — exactly the
//! fabrication `pdu.rs` documents as actively harmful. This decorator therefore
//! only resolves on the auto-commit path (`tx.is_none()`). A transactional
//! caller gets no silent pass: it must supply the graph fields itself, through
//! [`EventWriter::create_event_with_pdu`] or
//! [`EventWriter::create_event_with_graph`]. The two known callers are room
//! creation (`LifecycleService::write_creation_event`, which computes its own
//! linear graph metadata) and `MessagingService::create_event`, whose only
//! transactional caller (`send_message`, DB-03-a) writes just the event row and
//! its relation index — never room state — so its committed-state read is exact.
//!
//! # Failure policy
//!
//! Fail-closed. If the room version cannot be read, or a referenced `prev_event`
//! is missing, the write is rejected instead of being persisted with fabricated
//! graph fields. A missing room version means we cannot even pick the right
//! `auth_events` selection rules, and a `depth: 0` row makes peers file the
//! event as a DAG root.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use synapse_storage::event::{CreateEventParams, EventStorage, EventWriter, RoomEvent, StateEvent};
use synapse_storage::room::RoomStoreApi;

use crate::room::state::auth_events::{select_auth_events, AuthStateSnapshot};

/// How many forward extremities a new event references.
///
/// Matches the limit `sign_and_broadcast_event` already used when it built the
/// outbound PDU from a fresh query.
pub const FORWARD_EXTREMITY_LIMIT: i64 = 10;

/// The room version a newly-created room is being created *as*.
///
/// A create event establishes the version, so it comes from the event content;
/// when the caller states none, this server's own default is the decision (not a
/// guess about a remote room).
pub fn create_event_room_version(params: &CreateEventParams) -> String {
    params
        .content
        .get("room_version")
        .and_then(|value| value.as_str())
        .unwrap_or(synapse_common::room_versions::DEFAULT_ROOM_VERSION)
        .to_string()
}

/// The committed-state reads graph resolution needs.
///
/// Deliberately narrower than [`EventReader`]: three reads with fixed semantics
/// (real forward extremities, parent depths, current state) plus the room
/// version. That keeps the resolver testable against exact values instead of
/// against a mock's approximation of "latest events", and it documents the
/// contract each read has to honour.
#[async_trait]
pub trait GraphMetadataSource: Send + Sync {
    /// The room's current forward extremities, most recent first.
    async fn forward_extremities(&self, room_id: &str, limit: i64) -> Result<Vec<String>, sqlx::Error>;

    /// `event_id -> depth` for the requested events. Missing ids are omitted.
    async fn event_depths(&self, event_ids: &[String]) -> Result<HashMap<String, i64>, sqlx::Error>;

    /// The room's current state (one row per `(type, state_key)`).
    async fn state_events(&self, room_id: &str) -> Result<Vec<StateEvent>, sqlx::Error>;

    /// The room's version, if the room exists.
    async fn room_version(&self, room_id: &str) -> Result<Option<String>, sqlx::Error>;
}

/// [`GraphMetadataSource`] over Postgres `EventStorage` and the room store.
///
/// Holds the concrete storage type rather than `Arc<dyn EventReader>`: the
/// extremity read is a DAG query (`event_edges`), not one of the reader trait's
/// "latest events" reads, and the two must not be confused — the trait's
/// timestamp-ordered read would report an *ancestor* as a tip and put it in
/// `prev_events`.
pub struct StorageGraphMetadataSource {
    events: Arc<EventStorage>,
    rooms: Arc<dyn RoomStoreApi>,
}

impl StorageGraphMetadataSource {
    /// Builds the source over the storage readers.
    pub fn new(events: Arc<EventStorage>, rooms: Arc<dyn RoomStoreApi>) -> Self {
        Self { events, rooms }
    }
}

#[async_trait]
impl GraphMetadataSource for StorageGraphMetadataSource {
    async fn forward_extremities(&self, room_id: &str, limit: i64) -> Result<Vec<String>, sqlx::Error> {
        self.events.get_forward_extremities_in_room(room_id, limit).await
    }

    async fn event_depths(&self, event_ids: &[String]) -> Result<HashMap<String, i64>, sqlx::Error> {
        let events = self.events.get_events_map(event_ids).await?;
        Ok(events.into_iter().map(|(event_id, event)| (event_id, event.depth)).collect())
    }

    async fn state_events(&self, room_id: &str) -> Result<Vec<StateEvent>, sqlx::Error> {
        self.events.get_state_events(room_id).await
    }

    async fn room_version(&self, room_id: &str) -> Result<Option<String>, sqlx::Error> {
        self.rooms.get_room_version_only(room_id).await
    }
}

/// The graph fields a v3+ PDU requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventGraphMetadata {
    /// The room version the PDU must be shaped for. v1/v2 carry `event_id` as a
    /// PDU field; v3+ derive the ID from the reference hash and omit it.
    pub room_version: String,
    /// `prev_events` — the room's forward extremities at creation time.
    pub prev_events: Vec<String>,
    /// `auth_events` — the state events that authorise the sender.
    pub auth_events: Vec<String>,
    /// `depth` — one greater than the deepest `prev_event`.
    pub depth: i64,
}

impl EventGraphMetadata {
    /// The DAG-root projection: `m.room.create` has no parents and no auth
    /// events, and sits at depth 1.
    pub fn root(room_version: &str) -> Self {
        Self { room_version: room_version.to_string(), prev_events: Vec::new(), auth_events: Vec::new(), depth: 1 }
    }
}

/// Why graph metadata could not be resolved.
///
/// Every variant is a "refuse to write" condition: none has a safe fallback,
/// because substituting `[]` / `0` would misrepresent the event's position in
/// the room DAG.
#[derive(Debug, thiserror::Error)]
pub enum GraphMetadataError {
    /// The room has no row (or no `room_version`) — we cannot select auth events.
    #[error("room {room_id} has no room_version; refusing to guess auth_events")]
    UnknownRoomVersion {
        /// The room whose version is missing.
        room_id: String,
    },
    /// A non-create event in a room with no forward extremities.
    #[error("room {room_id} has no forward extremities; refusing to fabricate a DAG root")]
    NoForwardExtremities {
        /// The room with no extremities.
        room_id: String,
    },
    /// A `prev_event` referenced by the room's extremities is not readable.
    #[error("prev_event {event_id} is missing from storage; refusing to fabricate depth")]
    MissingPrevEvent {
        /// The unreadable parent event.
        event_id: String,
    },
    /// A read failed.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<GraphMetadataError> for sqlx::Error {
    fn from(error: GraphMetadataError) -> Self {
        match error {
            GraphMetadataError::Database(error) => error,
            refusal => sqlx::Error::Protocol(refusal.to_string()),
        }
    }
}

/// Computes [`EventGraphMetadata`] for an event about to be written.
#[derive(Clone)]
pub struct GraphMetadataResolver {
    source: Arc<dyn GraphMetadataSource>,
}

impl GraphMetadataResolver {
    /// Builds a resolver over a committed-state source.
    pub fn new(source: Arc<dyn GraphMetadataSource>) -> Self {
        Self { source }
    }

    /// The room's version, as the resolver's source reports it.
    pub async fn room_version(&self, room_id: &str) -> Result<Option<String>, GraphMetadataError> {
        Ok(self.source.room_version(room_id).await?)
    }

    /// Resolves the graph fields for `params`.
    pub async fn resolve(&self, params: &CreateEventParams) -> Result<EventGraphMetadata, GraphMetadataError> {
        // The create event is the DAG root by definition: it has no parents and
        // no auth events, and querying for them would demand exactly the state
        // the event itself establishes.
        if params.event_type == "m.room.create" {
            // A create event *establishes* the room version, so it comes from the
            // event content (falling back to this server's own default when the
            // caller did not state one — that is a local decision, not a guess
            // about a remote room).
            return Ok(EventGraphMetadata::root(&create_event_room_version(params)));
        }

        let room_version = self
            .source
            .room_version(&params.room_id)
            .await?
            .ok_or_else(|| GraphMetadataError::UnknownRoomVersion { room_id: params.room_id.clone() })?;

        // Forward extremities. The event may already be present on a retry of
        // the same write, in which case it must not be its own parent.
        let mut prev_events = self.source.forward_extremities(&params.room_id, FORWARD_EXTREMITY_LIMIT).await?;
        prev_events.retain(|event_id| event_id != &params.event_id);

        if prev_events.is_empty() {
            return Err(GraphMetadataError::NoForwardExtremities { room_id: params.room_id.clone() });
        }

        // depth = 1 + max(depth of prev_events). A parent we cannot read means
        // the DAG is not walkable; refuse rather than write a wrong depth.
        let depths = self.source.event_depths(&prev_events).await?;
        let mut max_depth: i64 = 0;
        for parent_id in &prev_events {
            let depth = depths
                .get(parent_id)
                .ok_or_else(|| GraphMetadataError::MissingPrevEvent { event_id: parent_id.clone() })?;
            max_depth = max_depth.max(*depth);
        }

        let state_events = self.source.state_events(&params.room_id).await?;
        let snapshot = AuthStateSnapshot::from_state_events(&state_events);
        let auth_events = select_auth_events(
            &room_version,
            &snapshot,
            &params.event_type,
            params.state_key.as_deref(),
            &params.user_id,
            &params.content,
        );

        Ok(EventGraphMetadata { room_version, prev_events, auth_events, depth: max_depth + 1 })
    }
}

/// Decorates an [`EventWriter`] so that locally-produced events are persisted
/// with their DAG metadata.
///
/// `create_event` resolves the graph fields and delegates to the inner writer's
/// `create_event_with_graph`; every other method is a straight pass-through, and
/// `create_event_with_graph` itself is **not** re-resolved — callers of that
/// method (inbound federation, backfill, room creation) already hold the
/// origin/known graph data and must stay byte-faithful to it.
pub struct GraphMetadataWriter {
    inner: Arc<dyn EventWriter>,
    resolver: Arc<GraphMetadataResolver>,
    server_name: String,
}

impl GraphMetadataWriter {
    /// Wraps `inner`, resolving graph metadata through `resolver`.
    ///
    /// `server_name` is this server's Matrix name — the PDU's `origin`, which
    /// participates in the event ID for room versions that still protect it.
    pub fn new(inner: Arc<dyn EventWriter>, resolver: Arc<GraphMetadataResolver>, server_name: String) -> Self {
        Self { inner, resolver, server_name }
    }

    /// U-13 step 2: replace the caller's placeholder ID with the reference-hash
    /// ID for v3+ (v1/v2 keep their server-assigned ID).
    ///
    /// Returns the params carrying the final ID. `hashes` are *not* written
    /// here: the broadcast path (`federation_broadcast::sign_and_broadcast_event`)
    /// computes and persists them, and both paths assemble the PDU with the same
    /// function (`synapse_common::pdu::build_pdu`), so the value it stores is the
    /// one this ID was derived from. The ID itself must be final *before* the row
    /// is inserted — a caller can hand it to a client in the same request.
    fn finalize_event_id(
        &self,
        mut params: CreateEventParams,
        room_version: &str,
        depth: i64,
        prev_events: &[String],
        auth_events: &[String],
    ) -> Result<CreateEventParams, sqlx::Error> {
        let parts = synapse_common::pdu::PduParts {
            room_version,
            event_id: Some(params.event_id.as_str()),
            room_id: &params.room_id,
            sender: &params.user_id,
            event_type: &params.event_type,
            content: &params.content,
            state_key: params.state_key.as_deref(),
            origin_server_ts: params.origin_server_ts,
            origin: &self.server_name,
            depth,
            prev_events,
            auth_events,
            redacts: params.redacts.as_deref(),
        };

        let finalized = synapse_federation::event_finalize::finalize_local_pdu(&parts)
            .map_err(|error| sqlx::Error::Protocol(format!("failed to finalize event id: {error}")))?;
        params.event_id = finalized.event_id;
        Ok(params)
    }
}

impl std::fmt::Debug for GraphMetadataWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphMetadataWriter").finish_non_exhaustive()
    }
}

#[async_trait]
impl EventWriter for GraphMetadataWriter {
    fn pool(&self) -> &Arc<sqlx::PgPool> {
        self.inner.pool()
    }

    async fn create_event(
        &self,
        params: CreateEventParams,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error> {
        // Only the auto-commit path can be resolved against committed state —
        // see the module docs on transactions.
        if tx.is_some() {
            return self.inner.create_event(params, tx).await;
        }

        let graph = self.resolver.resolve(&params).await?;
        let params =
            self.finalize_event_id(params, &graph.room_version, graph.depth, &graph.prev_events, &graph.auth_events)?;
        self.inner.create_event_with_graph(params, &graph.prev_events, &graph.auth_events, graph.depth, None).await
    }

    async fn create_event_with_pdu(
        &self,
        params: CreateEventParams,
        pdu_graph: synapse_storage::PduGraphFields,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error> {
        // The v12+ creation path supplies the complete graph fields, so the ID
        // can be derived here exactly as on the resolved path. Only the
        // auto-commit case is finalized: inside a caller's transaction the
        // version read would race the transaction's own writes.
        if tx.is_some() {
            return self.inner.create_event_with_pdu(params, pdu_graph, tx).await;
        }

        let (Some(depth), Some(prev_events), Some(auth_events)) =
            (pdu_graph.depth, pdu_graph.prev_events.clone(), pdu_graph.auth_events.clone())
        else {
            return Err(sqlx::Error::Protocol(
                "create_event_with_pdu requires depth/prev_events/auth_events to derive a v3+ event id".to_string(),
            ));
        };

        let room_version =
            self.resolver.room_version(&params.room_id).await?.unwrap_or_else(|| create_event_room_version(&params));
        let params = self.finalize_event_id(params, &room_version, depth, &prev_events, &auth_events)?;
        self.inner.create_event_with_pdu(params, pdu_graph, tx).await
    }

    async fn update_event_signatures_and_hashes(
        &self,
        event_id: &str,
        signatures: &serde_json::Value,
        hashes: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        self.inner.update_event_signatures_and_hashes(event_id, signatures, hashes).await
    }

    async fn redact_event_content(&self, event_id: &str, redacted_by: Option<&str>) -> Result<(), sqlx::Error> {
        self.inner.redact_event_content(event_id, redacted_by).await
    }

    async fn create_event_with_graph(
        &self,
        params: CreateEventParams,
        prev_events: &[String],
        auth_events: &[String],
        depth: i64,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error> {
        // Already carries graph data: re-resolving would overwrite the origin's
        // values with ours.
        self.inner.create_event_with_graph(params, prev_events, auth_events, depth, tx).await
    }

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
    ) -> Result<(), sqlx::Error> {
        self.inner.save_event_signature(event_id, user_id, device_id, signature, key_id, algorithm, created_ts).await
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
        self.inner.report_event(event_id, room_id, reported_user_id, reporter_user_id, reason, score).await
    }

    async fn add_ephemeral_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
        content: &serde_json::Value,
        stream_id: i64,
    ) -> Result<(), sqlx::Error> {
        self.inner.add_ephemeral_event(room_id, user_id, event_type, content, stream_id).await
    }

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
    ) -> Result<(), sqlx::Error> {
        self.inner
            .upsert_ephemeral_event(room_id, user_id, event_type, content, stream_id, created_ts, expires_at)
            .await
    }

    async fn delete_ephemeral_event(&self, room_id: &str, event_type: &str, user_id: &str) -> Result<(), sqlx::Error> {
        self.inner.delete_ephemeral_event(room_id, event_type, user_id).await
    }

    async fn delete_remote_events_before(
        &self,
        room_id: &str,
        timestamp: i64,
        dry_run: bool,
    ) -> Result<u64, sqlx::Error> {
        self.inner.delete_remote_events_before(room_id, timestamp, dry_run).await
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
        self.inner.upsert_power_levels_event(event_id, room_id, user_id, content, origin_server_ts, sender).await
    }

    async fn record_event_txn(
        &self,
        user_id: &str,
        room_id: &str,
        txn_id: &str,
        event_id: &str,
    ) -> Result<bool, sqlx::Error> {
        self.inner.record_event_txn(user_id, room_id, txn_id, event_id).await
    }

    async fn mark_event_soft_failed(&self, event_id: &str) -> Result<(), sqlx::Error> {
        self.inner.mark_event_soft_failed(event_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use synapse_common::current_timestamp_millis;
    use synapse_storage::test_mocks::event::InMemoryEventStore;

    const ROOM: &str = "!r:example.com";
    const ALICE: &str = "@alice:example.com";

    /// A controllable committed-state source: no mock approximation of
    /// "latest events" is involved, so expectations are exact.
    struct FakeSource {
        version: Option<String>,
        extremities: Vec<String>,
        depths: HashMap<String, i64>,
        state: Vec<StateEvent>,
    }

    impl FakeSource {
        fn room(version: &str, extremities: &[(&str, i64)]) -> Self {
            Self {
                version: Some(version.to_string()),
                extremities: extremities.iter().map(|(id, _)| id.to_string()).collect(),
                depths: extremities.iter().map(|(id, depth)| (id.to_string(), *depth)).collect(),
                state: vec![
                    state_event("$create", "m.room.create", ""),
                    state_event("$pl", "m.room.power_levels", ""),
                    state_event("$alice-member", "m.room.member", ALICE),
                ],
            }
        }
    }

    fn state_event(event_id: &str, event_type: &str, state_key: &str) -> StateEvent {
        StateEvent {
            event_id: event_id.to_string(),
            room_id: ROOM.to_string(),
            sender: ALICE.to_string(),
            event_type: Some(event_type.to_string()),
            content: json!({}),
            state_key: Some(state_key.to_string()),
            unsigned: None,
            is_redacted: Some(false),
            origin_server_ts: 1_000,
            depth: None,
            processed_ts: None,
            not_before: None,
            status: None,
            origin: None,
            user_id: None,
            stream_ordering: None,
            prev_events: None,
            auth_events: None,
            signatures: None,
            hashes: None,
        }
    }

    #[async_trait]
    impl GraphMetadataSource for FakeSource {
        async fn forward_extremities(&self, _room_id: &str, limit: i64) -> Result<Vec<String>, sqlx::Error> {
            Ok(self.extremities.iter().take(limit as usize).cloned().collect())
        }

        async fn event_depths(&self, event_ids: &[String]) -> Result<HashMap<String, i64>, sqlx::Error> {
            Ok(event_ids.iter().filter_map(|id| self.depths.get(id).map(|d| (id.clone(), *d))).collect())
        }

        async fn state_events(&self, _room_id: &str) -> Result<Vec<StateEvent>, sqlx::Error> {
            Ok(self.state.clone())
        }

        async fn room_version(&self, _room_id: &str) -> Result<Option<String>, sqlx::Error> {
            Ok(self.version.clone())
        }
    }

    fn params(event_type: &str, state_key: Option<&str>, content: serde_json::Value) -> CreateEventParams {
        CreateEventParams {
            event_id: format!("$new-{}:example.com", current_timestamp_millis()),
            room_id: ROOM.to_string(),
            user_id: ALICE.to_string(),
            event_type: event_type.to_string(),
            content,
            state_key: state_key.map(str::to_string),
            origin_server_ts: current_timestamp_millis(),
            redacts: None,
        }
    }

    fn resolver(source: FakeSource) -> GraphMetadataResolver {
        GraphMetadataResolver::new(Arc::new(source))
    }

    #[tokio::test]
    async fn create_event_resolves_to_the_dag_root() {
        let graph = resolver(FakeSource::room("11", &[("$e2", 7)]))
            .resolve(&params("m.room.create", Some(""), json!({"room_version": "11"})))
            .await
            .expect("resolve");
        assert_eq!(graph, EventGraphMetadata::root("11"));
    }

    #[tokio::test]
    async fn message_resolves_prev_depth_and_auth_events() {
        let graph = resolver(FakeSource::room("11", &[("$e2", 7), ("$e1", 5)]))
            .resolve(&params("m.room.message", None, json!({"body": "hi", "msgtype": "m.text"})))
            .await
            .expect("resolve");

        assert_eq!(graph.prev_events, vec!["$e2".to_string(), "$e1".to_string()]);
        assert_eq!(graph.depth, 8, "depth must be 1 + max(parent depth)");
        assert_eq!(graph.auth_events, vec!["$create".to_string(), "$alice-member".to_string(), "$pl".to_string()]);
    }

    #[tokio::test]
    async fn the_event_being_written_is_never_its_own_parent() {
        let graph = resolver(FakeSource::room("11", &[("$new-depth", 3), ("$e1", 5)]))
            .resolve(&{
                let mut params = params("m.room.message", None, json!({}));
                // A retry of an already-persisted write must not create a self-loop.
                params.event_id = "$new-depth".to_string();
                params
            })
            .await
            .expect("resolve");
        assert_eq!(graph.prev_events, vec!["$e1".to_string()]);
        assert_eq!(graph.depth, 6);
    }

    #[tokio::test]
    async fn unknown_room_version_fails_closed() {
        let mut source = FakeSource::room("11", &[("$e1", 5)]);
        source.version = None;
        let error =
            resolver(source).resolve(&params("m.room.message", None, json!({}))).await.expect_err("must refuse");
        assert!(matches!(error, GraphMetadataError::UnknownRoomVersion { .. }), "unexpected error: {error:?}");
    }

    #[tokio::test]
    async fn no_forward_extremities_fails_closed() {
        let error = resolver(FakeSource::room("11", &[]))
            .resolve(&params("m.room.message", None, json!({})))
            .await
            .expect_err("must refuse: no extremities to point at");
        assert!(matches!(error, GraphMetadataError::NoForwardExtremities { .. }), "unexpected error: {error:?}");
    }

    #[tokio::test]
    async fn unreadable_parent_fails_closed() {
        let mut source = FakeSource::room("11", &[("$e1", 5)]);
        source.depths.clear(); // extremity listed but its row is unreadable
        let error =
            resolver(source).resolve(&params("m.room.message", None, json!({}))).await.expect_err("must refuse");
        assert!(matches!(error, GraphMetadataError::MissingPrevEvent { .. }), "unexpected error: {error:?}");
    }

    /// The decorator must route `create_event` through the *graph* write: the
    /// in-memory writer stamps `status: Some("processed")` / `depth: 0` on the
    /// plain path, so a resolved depth proves the graph path was taken.
    #[tokio::test]
    async fn decorator_persists_resolved_graph_metadata() {
        let inner: Arc<dyn EventWriter> = Arc::new(InMemoryEventStore::new());
        let decorated: Arc<dyn EventWriter> = Arc::new(GraphMetadataWriter::new(
            inner,
            Arc::new(resolver(FakeSource::room("11", &[("$e1", 7)]))),
            "example.com".to_string(),
        ));

        let written = decorated
            .create_event(params("m.room.message", None, json!({"body": "hi", "msgtype": "m.text"})), None)
            .await
            .expect("write through the decorator");

        assert_eq!(written.depth, 8, "resolved depth must reach storage, got {}", written.depth);
        assert_eq!(written.status, None, "the plain create_event path would have stamped status=processed");
    }

    /// U-13 step 2 (acceptance items 4 and 6): the auto-commit write path assigns
    /// the **reference-hash** ID for v3+ — and that ID must equal what a peer
    /// recomputes from the PDU we later emit — while v1/v2 keep the caller's
    /// server-assigned ID.
    #[tokio::test]
    async fn decorator_assigns_the_reference_hash_event_id_for_v3_plus() {
        for (version, replaces_placeholder) in [("12", true), ("11", true), ("10", true), ("4", true), ("2", false)] {
            let inner: Arc<dyn EventWriter> = Arc::new(InMemoryEventStore::new());
            let decorated: Arc<dyn EventWriter> = Arc::new(GraphMetadataWriter::new(
                inner,
                Arc::new(resolver(FakeSource::room(version, &[("$e1", 7)]))),
                "example.com".to_string(),
            ));

            let placeholder = format!("$placeholder-{version}:example.com");
            let mut request = params("m.room.message", None, json!({"body": "hi", "msgtype": "m.text"}));
            request.event_id = placeholder.clone();

            let written = decorated.create_event(request.clone(), None).await.expect("write");

            if !replaces_placeholder {
                assert_eq!(written.event_id, placeholder, "v{version} keeps the server-assigned id");
                continue;
            }

            assert_ne!(written.event_id, placeholder, "v{version} must replace the placeholder");
            assert!(written.event_id.starts_with('$'));
            assert!(
                !written.event_id.contains(':'),
                "v{version}: reference-hash IDs carry no origin suffix: {}",
                written.event_id
            );

            // The receiver's view: rebuild the PDU from the room's graph plus the
            // event's own fields, and recompute the ID from it.
            let graph = resolver(FakeSource::room(version, &[("$e1", 7)])).resolve(&request).await.expect("resolve");
            let parts = synapse_common::pdu::PduParts {
                room_version: version,
                event_id: None,
                room_id: &written.room_id,
                sender: &written.user_id,
                event_type: &written.event_type,
                content: &written.content,
                state_key: written.state_key.as_deref(),
                origin_server_ts: written.origin_server_ts,
                origin: "example.com",
                depth: graph.depth,
                prev_events: &graph.prev_events,
                auth_events: &graph.auth_events,
                redacts: None,
            };
            let finalized = synapse_federation::event_finalize::finalize_local_pdu(&parts).expect("finalize");
            assert_eq!(
                written.event_id, finalized.event_id,
                "v{version}: the stored ID must equal the reference hash a peer recomputes from the emitted PDU"
            );
        }
    }

    /// `create_event_with_graph` must stay byte-faithful: the decorator may not
    /// overwrite graph data an inbound / backfill / room-creation caller supplied.
    #[tokio::test]
    async fn decorator_passes_supplied_graph_data_through() {
        let inner: Arc<dyn EventWriter> = Arc::new(InMemoryEventStore::new());
        let decorated: Arc<dyn EventWriter> = Arc::new(GraphMetadataWriter::new(
            inner,
            Arc::new(resolver(FakeSource::room("11", &[("$e1", 7)]))),
            "example.com".to_string(),
        ));

        let written = decorated
            .create_event_with_graph(
                params("m.room.message", None, json!({"body": "remote"})),
                &["$remote-prev".to_string()],
                &["$remote-auth".to_string()],
                42,
                None,
            )
            .await
            .expect("write");

        assert_eq!(written.depth, 42, "supplied depth must not be recomputed");
    }
}

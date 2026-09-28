//! The room's **resolved-state record** — the write half of F-1's MSC4297 wiring.
//!
//! `state_groups` / `state_group_state` have been in the schema all along
//! (`migrations/00000000_unified_schema_v12.sql`), but no production code ever
//! wrote them (see `docs/audit/ROOM_V12_PLAN_STATUS_2026-09-27.md` §4.6), and the
//! current-state read path was a pure `origin_server_ts` last-write-wins
//! derivation. §4.9 landed the **read** side: `EventStorage::get_state_events` /
//! `get_state_event` prefer a room's newest state group when one exists. This
//! module is what creates and maintains it.
//!
//! It runs **after a state event is committed**, from the two places that persist
//! state events:
//!
//! * [`MessagingService::create_event_with_graph`](crate::room::messaging) — the
//!   seam shared by local sends, the inbound federation transaction, backfill and
//!   invites;
//! * the federated-join state batch (`MembershipService`), which writes its state
//!   events through the storage writer in one transaction and therefore does not
//!   pass through the first seam.
//!
//! Three outcomes:
//!
//! * the room now has **more than one forward extremity** (a fork): the branches'
//!   state sets are derived from the persisted DAG and resolved with state
//!   resolution **v2.1** — [`EventAuthChain::resolve_state_for_version_with_rules`]
//!   (MSC4297, room v12+; v1–v11 keep v2) — and the result is written as the
//!   room's current record;
//! * otherwise, if the room already has a record: the new event is folded into it
//!   (**copy-forward**), so a later write cannot leave the record stale (the read
//!   path treats the newest group as the whole current state);
//! * otherwise (no fork, no record): nothing happens. The room stays on the
//!   event-log derivation, which agrees with resolution as long as the DAG never
//!   forked.
//!
//! Only **state** events pay for this — messages return before the first query, so
//! the hot message path is untouched.
//!
//! ## Deliberate simplifications (and where they would be revisited)
//!
//! * State-at-event is derived by walking `prev_events` per resolution instead of
//!   persisting a state group per event. The walk is bounded
//!   ([`MAX_RESOLUTION_EVENTS`]) and only runs while a room is forked, but a
//!   per-event state group (the `event_to_state_groups` table, today unused) is the
//!   scalable form.
//! * Maintenance is **best-effort**: it runs after the event is committed, so a
//!   failure cannot roll the event back. It degrades to the previous behaviour
//!   (the timestamp derivation) and logs, exactly like the signature/hash
//!   enrichment next to it.

use crate::common::error::{ApiError, ApiResult};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use synapse_common::current_timestamp_millis;
use synapse_federation::event_auth::{EventAuthChain, EventData};
use synapse_storage::event::{EventReader, RoomEvent};
use synapse_storage::room::RoomStoreApi;
use synapse_storage::state_groups::{StateGroupStateEntry, StateGroupStorage};

/// How many forward extremities a resolution will consider.
///
/// A fork wider than this is not resolved; the bound exists so a pathological DAG
/// cannot make the walk unbounded.
const EXTREMITY_LIMIT: i64 = 64;

/// Upper bound on the events loaded for one resolution (state sets plus their
/// transitive `auth_events`).
const MAX_RESOLUTION_EVENTS: usize = 4096;

/// The collaborators the record maintenance needs.
///
/// Taken explicitly rather than stored on a service so both write seams can use
/// one implementation without either service having to own the others' handles.
pub(crate) struct StateRecord<'a> {
    pub(crate) event_reader: &'a dyn EventReader,
    pub(crate) room_storage: &'a dyn RoomStoreApi,
    pub(crate) state_groups: &'a StateGroupStorage,
}

impl StateRecord<'_> {
    /// Maintain the room's resolved-state record after `event_id` (a state event
    /// with `(event_type, state_key)`) has been committed.
    ///
    /// Callers must only pass **committed** state events: the walk reads the DAG
    /// through the pool and cannot see a row that is still inside an uncommitted
    /// transaction.
    pub(crate) async fn after_state_event(
        &self,
        room_id: &str,
        event_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> ApiResult<()> {
        let extremities =
            self.event_reader.get_forward_extremities_in_room(room_id, EXTREMITY_LIMIT).await.map_err(|error| {
                ApiError::internal_with_cause("Failed to read the room's forward extremities", error)
            })?;

        if extremities.len() > 1 {
            return self.resolve_forked_state(room_id, event_id, &extremities).await;
        }

        self.copy_forward(room_id, event_id, event_type, state_key).await
    }

    /// Resolve the branches' state sets with v2.1 and write the result as the
    /// room's current record.
    async fn resolve_forked_state(&self, room_id: &str, event_id: &str, extremities: &[String]) -> ApiResult<()> {
        let room_version = self
            .room_storage
            .get_room_version_only(room_id)
            .await
            .map_err(|error| ApiError::internal_with_cause("Failed to read the room version", error))?
            .ok_or_else(|| {
                ApiError::internal(format!(
                    "room {room_id} has no recorded room version; refusing to resolve its state"
                ))
            })?;

        let mut walker = StateWalker::new(self.event_reader, &room_version);
        let mut state_sets: Vec<HashMap<String, Value>> = Vec::with_capacity(extremities.len());
        for extremity in extremities {
            state_sets.push(walker.state_at(extremity).await?);
        }
        let resolved = walker.resolve(&state_sets).await?;

        let entries = entries_from_resolved(&resolved);
        if entries.is_empty() {
            // Nothing survived the replay. Leaving the derivation in place is
            // strictly more useful than recording an empty state.
            return Ok(());
        }

        let mut prev_groups: Vec<i64> = Vec::new();
        for extremity in extremities {
            if let Some(group) = self
                .state_groups
                .get_state_group_for_event(extremity)
                .await
                .map_err(|error| ApiError::internal_with_cause("Failed to look up a state group", error))?
            {
                prev_groups.push(group);
            }
        }

        self.persist(room_id, event_id, &entries, &prev_groups).await
    }

    /// Fold one state event into an existing record (the room is not forked, so
    /// the new event extends the single branch).
    async fn copy_forward(&self, room_id: &str, event_id: &str, event_type: &str, state_key: &str) -> ApiResult<()> {
        let Some(current) = self
            .state_groups
            .get_room_state_groups(room_id, 1)
            .await
            .map_err(|error| ApiError::internal_with_cause("Failed to read the room's state record", error))?
            .into_iter()
            .next()
        else {
            // No record yet: the event log stays authoritative. Creating a record
            // here would mean materialising the whole state on every write.
            return Ok(());
        };

        // The read path serves a group's own rows (it does not walk the edges), so
        // a copy-forward must materialise the **whole** current state, not a delta.
        let mut entries: Vec<StateGroupStateEntry> = self
            .state_groups
            .get_state_at_group(current.id)
            .await
            .map_err(|error| ApiError::internal_with_cause("Failed to read the room's current state", error))?
            .into_iter()
            .map(|row| StateGroupStateEntry {
                event_type: row.event_type,
                state_key: row.state_key,
                event_id: row.event_id,
            })
            .collect();
        entries.retain(|entry| !(entry.event_type == event_type && entry.state_key == state_key));
        entries.push(StateGroupStateEntry {
            event_type: event_type.to_string(),
            state_key: state_key.to_string(),
            event_id: event_id.to_string(),
        });

        self.persist(room_id, event_id, &entries, &[current.id]).await
    }

    /// Write `entries` as a new state group for the room, bound to `event_id`.
    async fn persist(
        &self,
        room_id: &str,
        event_id: &str,
        entries: &[StateGroupStateEntry],
        prev_groups: &[i64],
    ) -> ApiResult<()> {
        let state_hash = state_hash(room_id, entries);
        let group_id = self
            .state_groups
            .create_state_group(room_id, event_id, &state_hash, current_timestamp_millis())
            .await
            .map_err(|error| ApiError::internal_with_cause("Failed to create the resolved-state group", error))?;
        self.state_groups
            .set_state_entries(group_id, entries)
            .await
            .map_err(|error| ApiError::internal_with_cause("Failed to write the resolved state", error))?;
        if !prev_groups.is_empty() {
            self.state_groups
                .add_state_group_edges(group_id, prev_groups)
                .await
                .map_err(|error| ApiError::internal_with_cause("Failed to link the resolved-state group", error))?;
        }
        self.state_groups
            .bind_event_to_state_group(event_id, group_id)
            .await
            .map_err(|error| ApiError::internal_with_cause("Failed to bind the event to its state group", error))?;
        Ok(())
    }
}

/// The future [`StateWalker::state_at`] returns: the walk is recursive over
/// `prev_events`, so it needs one layer of indirection.
type StateAtFuture<'s> = Pin<Box<dyn Future<Output = ApiResult<HashMap<String, Value>>> + Send + 's>>;

/// Derives the state at an event by walking the persisted room DAG.
struct StateWalker<'a> {
    reader: &'a dyn EventReader,
    chain: EventAuthChain,
    room_version: String,
    /// `event_id -> state at that event` (only branch points need memoising, but
    /// memoising every visited node keeps the walk linear in the DAG).
    memo: HashMap<String, HashMap<String, Value>>,
    /// The `auth_events` closure of everything seen, for the resolver's conflicted
    /// state subgraph and auth difference.
    events: HashMap<String, EventData>,
}

impl<'a> StateWalker<'a> {
    fn new(reader: &'a dyn EventReader, room_version: &str) -> Self {
        Self {
            reader,
            chain: EventAuthChain::new(),
            room_version: room_version.to_string(),
            memo: HashMap::new(),
            events: HashMap::new(),
        }
    }

    /// The state at `event_id`: the state at its parents, plus itself when it is a
    /// state event. Multiple parents are resolved with v2.1 — the same entry point
    /// [`StateRecord::resolve_forked_state`] uses.
    ///
    /// Boxed because the walk is recursive over `prev_events`.
    fn state_at<'s>(&'s mut self, event_id: &'s str) -> StateAtFuture<'s> {
        Box::pin(async move {
            if let Some(cached) = self.memo.get(event_id) {
                return Ok(cached.clone());
            }

            let fields = self
                .reader
                .get_event_graph_fields(event_id)
                .await
                .map_err(|error| ApiError::internal_with_cause("Failed to read event graph fields", error))?
                .ok_or_else(|| {
                    ApiError::internal(format!(
                        "event {event_id} has no persisted graph metadata; cannot derive the state it carries"
                    ))
                })?;
            let prev_events = id_array(fields.prev_events.as_ref());

            let mut state = match prev_events.len() {
                0 => HashMap::new(),
                1 => self.state_at(&prev_events[0]).await?,
                _ => {
                    let mut children: Vec<HashMap<String, Value>> = Vec::with_capacity(prev_events.len());
                    for prev in &prev_events {
                        children.push(self.state_at(prev).await?);
                    }
                    self.resolve(&children).await?
                }
            };

            if let Some(event) = self
                .reader
                .get_event(event_id)
                .await
                .map_err(|error| ApiError::internal_with_cause("Failed to read an event", error))?
            {
                if let Some(state_key) = event.state_key.as_deref() {
                    let value = event_data(&event, Vec::new(), Vec::new(), event.depth).to_state_event_value();
                    state.insert(state_key_of(&event.event_type, state_key), value);
                }
            }

            self.memo.insert(event_id.to_string(), state.clone());
            Ok(state)
        })
    }

    /// Resolve several state sets with the room version's rules (v2.1 for v12+).
    async fn resolve(&mut self, state_sets: &[HashMap<String, Value>]) -> ApiResult<HashMap<String, Value>> {
        for state_set in state_sets {
            for value in state_set.values() {
                if let Some(id) = value.get("event_id").and_then(Value::as_str) {
                    self.load_event_and_auth_chain(id).await?;
                }
            }
        }

        let borrowed: Vec<HashMap<String, &Value>> =
            state_sets.iter().map(|set| set.iter().map(|(key, value)| (key.clone(), value)).collect()).collect();
        let refs: Vec<&HashMap<String, &Value>> = borrowed.iter().collect();
        Ok(self.chain.resolve_state_for_version_with_rules(&self.room_version, &refs, &self.events))
    }

    /// Load one event and its transitive `auth_events`, which the resolver needs
    /// for the conflicted state subgraph and the auth difference.
    async fn load_event_and_auth_chain(&mut self, event_id: &str) -> ApiResult<()> {
        let mut queue: VecDeque<String> = VecDeque::new();
        queue.push_back(event_id.to_string());

        while let Some(id) = queue.pop_front() {
            if self.events.contains_key(&id) {
                continue;
            }
            if self.events.len() >= MAX_RESOLUTION_EVENTS {
                return Err(ApiError::internal(format!(
                    "state resolution would need more than {MAX_RESOLUTION_EVENTS} events; refusing to walk the whole DAG"
                )));
            }

            let Some(event) = self
                .reader
                .get_event(&id)
                .await
                .map_err(|error| ApiError::internal_with_cause("Failed to read an event", error))?
            else {
                continue;
            };
            let fields = self
                .reader
                .get_event_graph_fields(&id)
                .await
                .map_err(|error| ApiError::internal_with_cause("Failed to read event graph fields", error))?;
            let (auth_events, prev_events, depth) = match fields {
                Some(fields) => (
                    id_array(fields.auth_events.as_ref()),
                    id_array(fields.prev_events.as_ref()),
                    fields.depth.unwrap_or(0),
                ),
                None => (Vec::new(), Vec::new(), 0),
            };

            for auth in &auth_events {
                if !self.events.contains_key(auth) {
                    queue.push_back(auth.clone());
                }
            }
            self.events.insert(id.clone(), event_data(&event, auth_events, prev_events, depth));
        }

        Ok(())
    }
}

/// `(event_type, state_key)` as the resolver's state-map key.
fn state_key_of(event_type: &str, state_key: &str) -> String {
    format!("{event_type}:{state_key}")
}

/// The event IDs of a persisted `prev_events` / `auth_events` column.
fn id_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|entries| entries.iter().filter_map(|entry| entry.as_str().map(ToString::to_string)).collect())
        .unwrap_or_default()
}

/// One persisted event in the shape state resolution consumes.
fn event_data(event: &RoomEvent, auth_events: Vec<String>, prev_events: Vec<String>, depth: i64) -> EventData {
    EventData {
        event_id: event.event_id.clone(),
        room_id: event.room_id.clone(),
        event_type: event.event_type.clone(),
        auth_events,
        prev_events,
        state_key: event.state_key.clone().map(Value::String),
        content: Some(event.content.clone()),
        sender: event.user_id.clone(),
        origin_server_ts: event.origin_server_ts,
        depth,
    }
}

/// The resolvable `(event_type, state_key) -> event_id` entries of a resolution
/// result, sorted so the state hash is deterministic.
///
/// Entries whose value carries no `event_id` are dropped: the read path serves
/// these rows as state, so an entry that cannot name its event is unusable.
fn entries_from_resolved(resolved: &HashMap<String, Value>) -> Vec<StateGroupStateEntry> {
    let mut entries: Vec<StateGroupStateEntry> = resolved
        .iter()
        .filter_map(|(key, value)| {
            let (event_type, state_key) = key.split_once(':')?;
            let event_id = value.get("event_id").and_then(Value::as_str)?;
            Some(StateGroupStateEntry {
                event_type: event_type.to_string(),
                state_key: state_key.to_string(),
                event_id: event_id.to_string(),
            })
        })
        .collect();
    entries.sort_by(|a, b| (&a.event_type, &a.state_key).cmp(&(&b.event_type, &b.state_key)));
    entries
}

/// The record's content hash.
///
/// `state_groups.state_hash` is **globally** unique (`uq_state_groups_hash`), and
/// `create_state_group` upserts on it without touching `room_id`, so the room has
/// to be part of the hash — two rooms with identical state must not share a group.
fn state_hash(room_id: &str, entries: &[StateGroupStateEntry]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(room_id.as_bytes());
    let mut sorted: Vec<&StateGroupStateEntry> = entries.iter().collect();
    sorted.sort_by(|a, b| (&a.event_type, &a.state_key).cmp(&(&b.event_type, &b.state_key)));
    for entry in sorted {
        hasher.update(b"\n");
        hasher.update(entry.event_type.as_bytes());
        hasher.update(b"\x1f");
        hasher.update(entry.state_key.as_bytes());
        hasher.update(b"\x1f");
        hasher.update(entry.event_id.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use std::sync::Arc;
    use synapse_storage::event::{CreateEventParams, EventStorage};
    use synapse_storage::room::RoomStorage;

    /// A real-baseline isolated schema (AGENTS R9), or `None` when no test
    /// database is configured (mirrors the other `synapse-services` DB tests).
    async fn test_pool() -> Option<Arc<sqlx::PgPool>> {
        match crate::test_utils::prepare_isolated_test_pool().await {
            Ok(pool) => Some(pool),
            Err(error) => {
                eprintln!("Skipping state-record test, test database unavailable: {error}");
                None
            }
        }
    }

    fn state_params(
        room_id: &str,
        event_id: &str,
        sender: &str,
        event_type: &str,
        state_key: &str,
        content: serde_json::Value,
        ts: i64,
    ) -> CreateEventParams {
        CreateEventParams {
            event_id: event_id.to_string(),
            room_id: room_id.to_string(),
            user_id: sender.to_string(),
            event_type: event_type.to_string(),
            content,
            state_key: Some(state_key.to_string()),
            origin_server_ts: ts,
            redacts: None,
        }
    }

    fn message_params(room_id: &str, event_id: &str, sender: &str, ts: i64) -> CreateEventParams {
        CreateEventParams {
            event_id: event_id.to_string(),
            room_id: room_id.to_string(),
            user_id: sender.to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({ "msgtype": "m.text", "body": event_id }),
            state_key: None,
            origin_server_ts: ts,
            redacts: None,
        }
    }

    /// The room's DAG: a v12 create, `@alice`'s join, then two conflicting
    /// `m.room.topic` events on sibling branches. Branch B is authored by a
    /// non-member **and** is newer by `origin_server_ts`, so the timestamp
    /// derivation and the auth rules disagree about the winner.
    async fn build_forked_room(storage: &EventStorage, room_id: &str) -> (String, String) {
        let now = current_timestamp_millis();
        let create = "$create:example.com";
        let alice_join = "$alice_join:example.com";
        let topic_a = "$topic_a:example.com";
        let topic_b = "$topic_b:example.com";

        storage
            .create_event_with_graph(
                state_params(
                    room_id,
                    create,
                    "@alice:example.com",
                    "m.room.create",
                    "",
                    serde_json::json!({ "creator": "@alice:example.com", "room_version": "12" }),
                    now,
                ),
                &[],
                &[],
                1,
                None,
            )
            .await
            .expect("create");
        storage
            .create_event_with_graph(
                state_params(
                    room_id,
                    alice_join,
                    "@alice:example.com",
                    "m.room.member",
                    "@alice:example.com",
                    serde_json::json!({ "membership": "join" }),
                    now + 1,
                ),
                &[create.to_string()],
                &[create.to_string()],
                2,
                None,
            )
            .await
            .expect("alice join");
        storage
            .create_event_with_graph(
                state_params(
                    room_id,
                    topic_a,
                    "@alice:example.com",
                    "m.room.topic",
                    "",
                    serde_json::json!({ "topic": "branch-a" }),
                    now + 2,
                ),
                &[alice_join.to_string()],
                &[create.to_string(), alice_join.to_string()],
                3,
                None,
            )
            .await
            .expect("topic a");
        storage
            .create_event_with_graph(
                state_params(
                    room_id,
                    topic_b,
                    "@mallory:example.com",
                    "m.room.topic",
                    "",
                    serde_json::json!({ "topic": "branch-b" }),
                    now + 10,
                ),
                &[alice_join.to_string()],
                &[create.to_string()],
                3,
                None,
            )
            .await
            .expect("topic b");

        (topic_a.to_string(), topic_b.to_string())
    }

    async fn create_room(room_storage: &RoomStorage, room_id: &str) {
        room_storage.create_room(room_id, "@alice:example.com", "invite", "12", false).await.expect("room row");
    }

    fn record<'a>(
        storage: &'a EventStorage,
        room_storage: &'a RoomStorage,
        state_groups: &'a StateGroupStorage,
    ) -> StateRecord<'a> {
        StateRecord { event_reader: storage, room_storage, state_groups }
    }

    /// A `MessagingService` wired to real, DB-backed storages — so the **service
    /// seam** can be exercised, not just [`StateRecord`] directly.
    fn messaging_service(pool: &Arc<sqlx::PgPool>) -> crate::room::messaging::service::MessagingService {
        use crate::room::messaging::service::MessagingServiceConfig;
        use crate::room::summary::RoomSummaryService;
        use synapse_cache::{CacheConfig, CacheManager};
        use synapse_storage::event::{EventReader, EventWriter};
        use synapse_storage::test_mocks::{InMemoryMemberStore, InMemoryRelationsStore, InMemoryRoomSummaryStore};

        let storage = Arc::new(EventStorage::new(pool, "example.com".to_string()));
        let event_reader: Arc<dyn EventReader> = storage.clone();
        let event_writer: Arc<dyn EventWriter> = storage.clone();
        crate::room::messaging::service::MessagingService::new(MessagingServiceConfig {
            event_reader: event_reader.clone(),
            event_writer,
            room_storage: Arc::new(RoomStorage { pool: pool.clone() }),
            member_storage: Arc::new(InMemoryMemberStore::new()),
            server_name: "example.com".to_string(),
            beacon_service: None,
            task_queue: None,
            relations_storage: Arc::new(InMemoryRelationsStore::new()),
            event_broadcaster: None,
            app_service_manager: None,
            key_rotation_manager: None,
            room_summary_service: Arc::new(RoomSummaryService {
                storage: Arc::new(InMemoryRoomSummaryStore::new()),
                event_reader,
                member_storage: Some(Arc::new(InMemoryMemberStore::new())),
            }),
            cache: Arc::new(CacheManager::new(&CacheConfig::default())),
        })
    }

    async fn topic_id(storage: &EventStorage, room_id: &str) -> Option<String> {
        storage.get_state_event(room_id, "m.room.topic", "").await.expect("read topic").map(|event| event.event_id)
    }

    /// A fork is resolved with v2.1: the **authorised** branch wins even though the
    /// rejected one is newer, and the read path then serves the record.
    #[tokio::test]
    async fn forked_state_is_resolved_and_served() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let storage = EventStorage::new(&pool, "example.com".to_string());
        let room_storage = RoomStorage { pool: pool.clone() };
        let state_groups = StateGroupStorage::new(&pool);
        let room_id = format!("!resolved_{}:example.com", uuid::Uuid::new_v4());
        create_room(&room_storage, &room_id).await;

        let (topic_a, topic_b) = build_forked_room(&storage, &room_id).await;

        // Before the record exists the timestamp derivation wins — and picks the
        // branch the auth rules reject. This is the behaviour the wiring replaces.
        assert_eq!(topic_id(&storage, &room_id).await, Some(topic_b.clone()), "no record ⇒ timestamp derivation");

        assert_eq!(
            storage.get_forward_extremities_in_room(&room_id, 64).await.expect("extremities").len(),
            2,
            "the two topic branches are the forward extremities"
        );

        record(&storage, &room_storage, &state_groups)
            .after_state_event(&room_id, &topic_b, "m.room.topic", "")
            .await
            .expect("maintaining the record must succeed");

        assert!(
            !state_groups.get_room_state_groups(&room_id, 1).await.expect("groups").is_empty(),
            "resolving a fork must create a state record"
        );
        assert_eq!(
            topic_id(&storage, &room_id).await,
            Some(topic_a.clone()),
            "the record must serve the authorised branch, not the newer timestamp"
        );

        // The record is the whole current state, not only the conflicted key.
        let state = storage.get_state_events(&room_id).await.expect("state");
        let types: Vec<&str> = state.iter().filter_map(|event| event.event_type.as_deref()).collect();
        assert!(types.contains(&"m.room.create"), "unconflicted keys must survive: {types:?}");
        assert!(types.contains(&"m.room.member"), "unconflicted keys must survive: {types:?}");
    }

    /// A later state event on an unforked room is folded into the existing record,
    /// so the record cannot go stale.
    #[tokio::test]
    async fn later_state_events_are_folded_into_the_record() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let storage = EventStorage::new(&pool, "example.com".to_string());
        let room_storage = RoomStorage { pool: pool.clone() };
        let state_groups = StateGroupStorage::new(&pool);
        let room_id = format!("!folded_{}:example.com", uuid::Uuid::new_v4());
        create_room(&room_storage, &room_id).await;

        let (topic_a, topic_b) = build_forked_room(&storage, &room_id).await;
        let record = record(&storage, &room_storage, &state_groups);
        record.after_state_event(&room_id, &topic_b, "m.room.topic", "").await.expect("resolve the fork");
        assert_eq!(topic_id(&storage, &room_id).await, Some(topic_a.clone()));

        // Merge the branches with a message (no state maintenance runs for it) so a
        // single tip remains, then write a topic that extends that tip.
        let now = current_timestamp_millis();
        let merge = "$merge:example.com";
        storage
            .create_event_with_graph(
                message_params(&room_id, merge, "@alice:example.com", now + 20),
                &[topic_a, topic_b],
                &[],
                4,
                None,
            )
            .await
            .expect("merge");
        assert_eq!(storage.get_forward_extremities_in_room(&room_id, 64).await.expect("tips"), vec![merge.to_string()]);

        let topic_c = "$topic_c:example.com";
        storage
            .create_event_with_graph(
                state_params(
                    &room_id,
                    topic_c,
                    "@alice:example.com",
                    "m.room.topic",
                    "",
                    serde_json::json!({ "topic": "branch-c" }),
                    now + 21,
                ),
                &[merge.to_string()],
                &[],
                5,
                None,
            )
            .await
            .expect("topic c");
        record.after_state_event(room_id.as_str(), topic_c, "m.room.topic", "").await.expect("fold forward");

        assert_eq!(
            topic_id(&storage, &room_id).await,
            Some(topic_c.to_string()),
            "the record must reflect the newest state event"
        );
        let state = storage.get_state_events(&room_id).await.expect("state");
        assert!(state.iter().any(|event| event.event_type.as_deref() == Some("m.room.create")));
        assert!(state.iter().any(|event| event.event_type.as_deref() == Some("m.room.member")));
    }

    /// A room that never forked gets no record: its state stays on the event-log
    /// derivation, i.e. the pre-existing behaviour is unchanged.
    #[tokio::test]
    async fn unforked_rooms_keep_the_event_log_derivation() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let storage = EventStorage::new(&pool, "example.com".to_string());
        let room_storage = RoomStorage { pool: pool.clone() };
        let state_groups = StateGroupStorage::new(&pool);
        let room_id = format!("!linear_{}:example.com", uuid::Uuid::new_v4());
        create_room(&room_storage, &room_id).await;

        let now = current_timestamp_millis();
        storage
            .create_event_with_graph(
                state_params(
                    &room_id,
                    "$create:example.com",
                    "@alice:example.com",
                    "m.room.create",
                    "",
                    serde_json::json!({ "creator": "@alice:example.com", "room_version": "12" }),
                    now,
                ),
                &[],
                &[],
                1,
                None,
            )
            .await
            .expect("create");
        storage
            .create_event_with_graph(
                state_params(
                    &room_id,
                    "$alice_join:example.com",
                    "@alice:example.com",
                    "m.room.member",
                    "@alice:example.com",
                    serde_json::json!({ "membership": "join" }),
                    now + 1,
                ),
                &["$create:example.com".to_string()],
                &["$create:example.com".to_string()],
                2,
                None,
            )
            .await
            .expect("join");
        storage
            .create_event_with_graph(
                state_params(
                    &room_id,
                    "$topic:example.com",
                    "@alice:example.com",
                    "m.room.topic",
                    "",
                    serde_json::json!({ "topic": "linear" }),
                    now + 2,
                ),
                &["$alice_join:example.com".to_string()],
                &[],
                3,
                None,
            )
            .await
            .expect("topic");

        record(&storage, &room_storage, &state_groups)
            .after_state_event(&room_id, "$topic:example.com", "m.room.topic", "")
            .await
            .expect("maintenance is a no-op here");

        assert!(
            state_groups.get_room_state_groups(&room_id, 1).await.expect("groups").is_empty(),
            "an unforked, record-less room must not gain a record"
        );
        assert_eq!(topic_id(&storage, &room_id).await, Some("$topic:example.com".to_string()));
    }

    /// **The wiring test**: writing a forked state event through the service seam
    /// (`MessagingService::create_event_with_graph`) must maintain the record, so
    /// the read path serves the resolved state without anyone calling
    /// [`StateRecord`] by hand.
    ///
    /// This is the evidence the plan demands for every MSC4297 item — "implemented
    /// but never called" is this repo's documented failure mode (plan §4.1).
    #[tokio::test]
    async fn the_service_write_seam_maintains_the_record() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let room_id = format!("!seam_{}:example.com", uuid::Uuid::new_v4());
        let room_storage = RoomStorage { pool: pool.clone() };
        create_room(&room_storage, &room_id).await;
        let service = messaging_service(&pool);

        let now = current_timestamp_millis();
        let create = "$create:example.com";
        let alice_join = "$alice_join:example.com";
        let topic_a = "$topic_a:example.com";
        let topic_b = "$topic_b:example.com";

        service
            .create_event_with_graph(
                state_params(
                    &room_id,
                    create,
                    "@alice:example.com",
                    "m.room.create",
                    "",
                    serde_json::json!({ "creator": "@alice:example.com", "room_version": "12" }),
                    now,
                ),
                &[],
                &[],
                1,
                None,
            )
            .await
            .expect("create");
        service
            .create_event_with_graph(
                state_params(
                    &room_id,
                    alice_join,
                    "@alice:example.com",
                    "m.room.member",
                    "@alice:example.com",
                    serde_json::json!({ "membership": "join" }),
                    now + 1,
                ),
                &[create.to_string()],
                &[create.to_string()],
                2,
                None,
            )
            .await
            .expect("join");
        service
            .create_event_with_graph(
                state_params(
                    &room_id,
                    topic_a,
                    "@alice:example.com",
                    "m.room.topic",
                    "",
                    serde_json::json!({ "topic": "branch-a" }),
                    now + 2,
                ),
                &[alice_join.to_string()],
                &[create.to_string(), alice_join.to_string()],
                3,
                None,
            )
            .await
            .expect("topic a");
        // The fork: a second, newer topic on a sibling branch, written through the
        // same seam. The maintenance runs inside `create_event_with_graph`.
        service
            .create_event_with_graph(
                state_params(
                    &room_id,
                    topic_b,
                    "@mallory:example.com",
                    "m.room.topic",
                    "",
                    serde_json::json!({ "topic": "branch-b" }),
                    now + 10,
                ),
                &[alice_join.to_string()],
                &[create.to_string()],
                3,
                None,
            )
            .await
            .expect("topic b");

        let storage = EventStorage::new(&pool, "example.com".to_string());
        assert!(
            !StateGroupStorage::new(&pool).get_room_state_groups(&room_id, 1).await.expect("groups").is_empty(),
            "writing the forking state event through the service must create the record"
        );
        assert_eq!(
            topic_id(&storage, &room_id).await,
            Some(topic_a.to_string()),
            "and the read path must serve the branch the auth rules authorise"
        );
    }
}

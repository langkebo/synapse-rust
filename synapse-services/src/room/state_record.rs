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
//! * otherwise (no fork, no record): the room's first state write **seeds** the
//!   record from the state the event-log derivation currently returns (A4). Before
//!   this existed, a room that never forked never got a record at all, so the read
//!   path stayed on `DISTINCT ON … origin_server_ts DESC` forever — and that
//!   derivation can disagree with the DAG order, because `origin_server_ts` is the
//!   sender's clock and federated events may arrive with an older timestamp.
//!   Seeding is one full state read **per room**, never per write.
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
use std::sync::{Arc, Mutex};
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

/// M-5: how many resolution results the cache keeps. Exceeding it clears the map
/// (bounded memory; correctness never depends on a hit).
const RESOLUTION_CACHE_CAPACITY: usize = 64;

/// M-5: the resolution-result cache, **shared across walks** and therefore across
/// requests.
///
/// Cloning shares the same map (it is an `Arc`), so a [`MessagingService`] and a
/// [`MembershipService`] handed the same value reuse each other's resolutions. A
/// room that stays forked sees a fresh walk per state event; each walk otherwise
/// reruns the whole conflicted-set replay, and the fan-in inside it repeats across
/// those walks, so the shared cache turns repeats into hits.
///
/// [`MessagingService`]: crate::room::messaging::service::MessagingService
/// [`MembershipService`]: crate::room::membership::service::MembershipService
// Public (not `pub(crate)`) only because the `*ServiceConfig` fields that carry
// it are public and external crates construct those configs. Its methods stay
// crate-private; external callers only ever `Default::default()` it.
#[derive(Clone, Default)]
pub struct ResolutionCache {
    inner: Arc<Mutex<ResolutionCacheInner>>,
}

#[derive(Default)]
struct ResolutionCacheInner {
    entries: HashMap<ResolutionCacheKey, HashMap<String, Value>>,
    /// HIT/MISS counters — the tests assert on them, so "the cache exists but is
    /// never consulted" cannot pass as done.
    hits: u64,
    misses: u64,
}

impl ResolutionCache {
    /// See [`ResolutionCache`].
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The cached resolution for `key`, or `None` on a miss. Updates the counters.
    ///
    /// The lock is released before returning, so callers never hold it across the
    /// resolution work (which is not `await`-free only because of the loads
    /// preceding it, done before this point).
    fn get(&self, key: &ResolutionCacheKey) -> Option<HashMap<String, Value>> {
        // The cache is best-effort: a poisoned lock (a panic elsewhere while
        // holding it) must not take down resolution, so recover the guard.
        let mut inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        // Clone out before touching the counters: the counters borrow `inner`
        // mutably, and the entry is immutably borrowed for the duration of the lookup.
        match inner.entries.get(key).cloned() {
            Some(resolved) => {
                inner.hits += 1;
                Some(resolved)
            }
            None => {
                inner.misses += 1;
                None
            }
        }
    }

    /// Store a resolution, clearing the map first when the bound is reached.
    fn put(&self, key: ResolutionCacheKey, resolved: HashMap<String, Value>) {
        let mut inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if inner.entries.len() >= RESOLUTION_CACHE_CAPACITY {
            inner.entries.clear();
        }
        inner.entries.insert(key, resolved);
    }

    /// `(hits, misses)` — the tests read these to prove the cache is consulted.
    #[cfg(test)]
    fn counts(&self) -> (u64, u64) {
        let inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        (inner.hits, inner.misses)
    }
}

/// The collaborators the record maintenance needs.
///
/// Taken explicitly rather than stored on a service so both write seams can use
/// one implementation without either service having to own the others' handles.
pub(crate) struct StateRecord<'a> {
    pub(crate) event_reader: &'a dyn EventReader,
    pub(crate) room_storage: &'a dyn RoomStoreApi,
    pub(crate) state_groups: &'a StateGroupStorage,
    /// M-5: the process-shared resolution cache (see [`ResolutionCache`]).
    pub(crate) resolution_cache: &'a ResolutionCache,
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

        let mut walker = StateWalker::new(self.event_reader, &room_version, self.resolution_cache);
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
            // A4（2026-10-17）：**首写播种**。
            //
            // 此前这里直接 `Ok(())`，理由是"没有记录就让事件日志当权威，建记录等于每次写都
            // 物化全量状态"。前半句对、后半句只在"每次写都播种"时才成立 —— 播种成功后房间
            // 就有记录了，之后一律走 copy-forward，代价是**每个房间一次**全量状态读。
            //
            // 不播种的后果（A4 的实质，不只是性能）：从未分叉的房间永远没有记录，读路径
            // 永远走 `DISTINCT ON … origin_server_ts DESC`。而 `origin_server_ts` 是发送方
            // 时钟，联邦事件可以带着偏旧的时间戳后到 —— 那条推导会给出**与 DAG 顺序相反**
            // 的当前状态。没有记录 ⇒ 这条路径永远关不掉。
            return self.seed_from_current_state(room_id, event_id).await;
        };

        // A3 IMPLEMENTATION: Check idempotency first
        // If this event is already bound to a state group, skip
        if let Some(existing_group) = self
            .state_groups
            .get_state_group_for_event(event_id)
            .await
            .map_err(|error| ApiError::internal_with_cause("Failed to check event binding", error))?
        {
            tracing::debug!(
                event_id = event_id,
                group_id = existing_group,
                "Event already bound to state group, skipping copy-forward"
            );
            return Ok(());
        }

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

    /// Seed a freshly created room's record.
    ///
    /// Room creation writes its whole initial state through
    /// `EventWriter::create_event_with_pdu` inside one transaction (see
    /// `lifecycle::create_events::write_creation_event`), so it never reaches
    /// [`Self::after_state_event`] — the seam that seeds every other room. Without
    /// this call the room has state events but no record, and (A1) every
    /// current-state read fails closed until the room's first post-creation
    /// **state** write: `GET /rooms/{id}/state`, `/messages`, `/event` and
    /// `/context` (via the history-visibility lookup) would all error on a brand
    /// new room.
    ///
    /// Idempotent: a room that already has a record (a retried creation, or the
    /// room whose first state write won the race) is left alone.
    pub(crate) async fn seed_new_room(&self, room_id: &str) -> ApiResult<()> {
        let already_seeded = !self
            .state_groups
            .get_room_state_groups(room_id, 1)
            .await
            .map_err(|error| ApiError::internal_with_cause("Failed to read the room's state record", error))?
            .is_empty();
        if already_seeded {
            return Ok(());
        }

        // The creation sequence is linear, so the single forward extremity is the
        // room's newest event — the anchor the record is bound to.
        let Some(tip) = self
            .event_reader
            .get_forward_extremities_in_room(room_id, 1)
            .await
            .map_err(|error| ApiError::internal_with_cause("Failed to read the room's forward extremities", error))?
            .into_iter()
            .next()
        else {
            // No events at all: nothing to record, same as an empty derivation.
            return Ok(());
        };

        self.seed_from_current_state(room_id, &tip).await
    }

    /// Seed the room's record from the state the event-log derivation currently
    /// returns (A4).
    ///
    /// One-shot per room: after this succeeds the room has a record, so every later
    /// state write takes the [`Self::copy_forward`] branch and the seed never runs
    /// again.
    ///
    /// **为什么用 `derive_state_from_event_log` 而不是 `get_state_events`**：后者是
    /// **fail-closed** 的读路径（A1）—— 房间没有记录时直接 `Err`，而这里恰恰就是"没有记录"
    /// 的那一次写入。若改调 `get_state_events`，新房间的首个状态写会永远失败（死锁式回归）。
    /// 推导入口保留的唯一理由就是播种本身："把推导的结果固化成记录"，之后所有变更由
    /// copy-forward 维护，推导不再被读。
    ///
    /// **为什么不用 `StateWalker::state_at`**：后者要沿
    /// `prev_events` 递归整条历史（大房间是 O(历史长度) 次查询），而这里只需要"当前状态"
    /// 这一张快照 —— 推导是一条按房间状态集大小的查询，与历史长度无关。
    ///
    /// **已知竞态（与 copy-forward 同类，不新增类别）**：两个并发的首个状态写都看不到记录
    /// 时，会各自播种一份；`create_state_group` 按 `state_hash` upsert、读路径取 `id` 最大的
    /// 组，因此后者胜出，先播种的那一份可能少一个并发事件。要彻底消除需要按房间串行化
    /// 状态写（上游也是这么做的），属独立改动。
    async fn seed_from_current_state(&self, room_id: &str, event_id: &str) -> ApiResult<()> {
        let derived = self
            .event_reader
            .derive_state_from_event_log(room_id)
            .await
            .map_err(|error| ApiError::internal_with_cause("Failed to read the room's current state", error))?;

        let entries: Vec<StateGroupStateEntry> = derived
            .into_iter()
            .filter_map(|event| {
                // 状态集只由 `(event_type, state_key)` 构成；任一缺失都不是状态事件。
                let state_key = event.state_key?;
                let event_type = event.event_type?;
                Some(StateGroupStateEntry { event_type, state_key, event_id: event.event_id })
            })
            .collect();

        if entries.is_empty() {
            // 连 create 都没有（异常房间）：留空记录比留错记录安全，继续走推导。
            return Ok(());
        }

        self.persist(room_id, event_id, &entries, &[]).await
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
    /// M-5: the process-shared **resolution-result** cache, keyed by every input the
    /// resolver reads. A fork's fan-in repeats across branches (and across the walks
    /// of a room that stays forked), and each repeat otherwise reruns the whole
    /// conflicted-set replay.
    ///
    /// ⚠️ The key must cover **all** inputs: a key that misses one returns a *wrong*
    /// resolved state, which is strictly worse than not caching. Because
    /// `load_event_and_auth_chain` only ever inserts (it refuses to overwrite an id),
    /// the id set identifies the `events` map's contents inside one walker.
    cache: &'a ResolutionCache,
}

/// M-5 cache key: `(room version, state sets' sorted `(key, event_id)` projection,
/// sorted loaded event ids)`. The room version is a slot because the cache is shared
/// across rooms, whose versions can differ.
type ResolutionCacheKey = (String, Vec<Vec<(String, String)>>, Vec<String>);

impl<'a> StateWalker<'a> {
    fn new(reader: &'a dyn EventReader, room_version: &str, cache: &'a ResolutionCache) -> Self {
        Self {
            reader,
            chain: EventAuthChain::new(),
            room_version: room_version.to_string(),
            memo: HashMap::new(),
            events: HashMap::new(),
            cache,
        }
    }

    /// M-5: the key described on [`Self::cache`]. State sets are sorted so that the
    /// order in which branches were walked cannot split the cache.
    fn resolution_cache_key(&self, state_sets: &[HashMap<String, Value>]) -> ResolutionCacheKey {
        let mut projection: Vec<Vec<(String, String)>> = state_sets
            .iter()
            .map(|set| {
                let mut pairs: Vec<(String, String)> = set
                    .iter()
                    .map(|(key, value)| {
                        let event_id = value.get("event_id").and_then(Value::as_str).unwrap_or_default().to_string();
                        (key.clone(), event_id)
                    })
                    .collect();
                pairs.sort();
                pairs
            })
            .collect();
        projection.sort();

        let mut loaded: Vec<String> = self.events.keys().cloned().collect();
        loaded.sort();
        (self.room_version.clone(), projection, loaded)
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

        // M-5: the loads above are idempotent (`load_event_and_auth_chain` skips ids
        // it already has), so an identical fan-in reaches this point having done no
        // extra I/O — only the resolution itself is left to save. The cache is
        // process-shared, so a repeat that arrived on an earlier request is also a hit.
        let key = self.resolution_cache_key(state_sets);
        if let Some(cached) = self.cache.get(&key) {
            return Ok(cached);
        }

        let borrowed: Vec<HashMap<String, &Value>> =
            state_sets.iter().map(|set| set.iter().map(|(key, value)| (key.clone(), value)).collect()).collect();
        let refs: Vec<&HashMap<String, &Value>> = borrowed.iter().collect();
        let resolved = self.chain.resolve_state_for_version_with_rules(&self.room_version, &refs, &self.events);
        self.cache.put(key, resolved.clone());
        Ok(resolved)
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
    hex_lower(&hasher.finalize())
}

/// 小写十六进制（无前缀、无分隔符）—— `member_state_hash` 的输出是**持久化**的，
/// 格式必须逐字符稳定：`digest 0.11` 起 `finalize()` 返回 `hybrid_array::Array`，
/// 不再实现 `LowerHex`，所以原来那句 `format!("{:x}", ...)` 从"能编译"变成"编不过"。
/// 顺手改成显式逐字节格式化，避免有人用 `.as_slice()` 之类的变形蒙过编译而改了输出。
fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
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
        resolution_cache: &'a ResolutionCache,
    ) -> StateRecord<'a> {
        StateRecord { event_reader: storage, room_storage, state_groups, resolution_cache }
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
            resolution_cache: ResolutionCache::new(),
            event_admission_gate: Arc::new(crate::test_mocks::FakeEventAdmissionGate::new()),
        })
    }

    async fn topic_id(storage: &EventStorage, room_id: &str) -> Option<String> {
        storage.get_state_event(room_id, "m.room.topic", "").await.expect("read topic").map(|event| event.event_id)
    }

    /// The topic according to the raw event-log derivation — what the read path
    /// answered before the record existed (and what `SYNAPSE_ALLOW_STATE_TS_FALLBACK`
    /// restores). Used to show *why* the DAG order and the sender clock disagree.
    async fn derived_topic_id(storage: &EventStorage, room_id: &str) -> Option<String> {
        storage
            .derive_state_from_event_log(room_id)
            .await
            .expect("derive")
            .into_iter()
            .find(|event| event.event_type.as_deref() == Some("m.room.topic") && event.state_key.as_deref() == Some(""))
            .map(|event| event.event_id)
    }

    /// M-5: 决议**结果缓存**。键覆盖全部输入（房间版本 + 状态集合投影 + 已加载事件集合），
    /// 因此：同一冲突集合第二次必须命中且**返回同一状态图**；输入变化必须 MISS。
    ///
    /// 这条用例同时是"缓存没有被静默旁路"的证明 —— 只断言结果相等是不够的，
    /// 没命中的缓存同样会返回正确结果。
    #[tokio::test]
    async fn resolution_cache_hits_on_the_same_fork_and_misses_when_inputs_change() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let storage = EventStorage::new(&pool, "example.com".to_string());
        let room_storage = RoomStorage { pool: pool.clone() };
        let room_id = format!("!rescache_{}:example.com", uuid::Uuid::new_v4());
        create_room(&room_storage, &room_id).await;

        let (topic_a, topic_b) = build_forked_room(&storage, &room_id).await;

        let cache = ResolutionCache::new();
        let mut walker = StateWalker::new(&storage, "12", &cache);
        let set_a = walker.state_at(&topic_a).await.expect("state at branch A");
        let set_b = walker.state_at(&topic_b).await.expect("state at branch B");

        let first = walker.resolve(&[set_a.clone(), set_b.clone()]).await.expect("first resolve");
        assert_eq!(cache.counts(), (0, 1), "首次决议必须是 MISS");

        let second = walker.resolve(&[set_a.clone(), set_b.clone()]).await.expect("second resolve");
        assert_eq!(cache.counts(), (1, 1), "同一冲突集合第二次必须命中缓存（否则这个缓存等于不存在）");
        assert_eq!(first, second, "HIT 与 MISS 必须返回同一状态图：键漏一个输入就会返回错误状态");

        // 分支遍历顺序不是输入语义：交换状态集合顺序后必须仍然命中。
        let reordered = walker.resolve(&[set_b.clone(), set_a.clone()]).await.expect("reordered resolve");
        assert_eq!(cache.counts(), (2, 1), "状态集合顺序不应造成 MISS（键按排序归一）");
        assert_eq!(first, reordered, "交换顺序不得改变结果");

        // 输入集合本身变化 ⇒ 必须 MISS（键覆盖状态集合投影）。
        walker.resolve(std::slice::from_ref(&set_a)).await.expect("single set");
        assert_eq!(cache.counts(), (2, 2), "只传一个状态集合是不同输入，必须 MISS");
    }

    /// M-5（跨请求共享）：两次**各自新建**的 walk（模拟两个请求）共用同一个
    /// [`ResolutionCache`]，第二次的同一冲突集合必须命中第一次写入的条目 —— 证明缓存
    /// 的生命周期不再被单次 walk（单次请求）限制。
    #[tokio::test]
    async fn resolution_cache_is_shared_across_walks() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let storage = EventStorage::new(&pool, "example.com".to_string());
        let room_storage = RoomStorage { pool: pool.clone() };
        let room_id = format!("!rescache_shared_{}:example.com", uuid::Uuid::new_v4());
        create_room(&room_storage, &room_id).await;

        let (topic_a, topic_b) = build_forked_room(&storage, &room_id).await;

        let cache = ResolutionCache::new();

        // 请求 1：一次 walk 决议该分叉。
        let mut first_walk = StateWalker::new(&storage, "12", &cache);
        let set_a = first_walk.state_at(&topic_a).await.expect("state at branch A");
        let set_b = first_walk.state_at(&topic_b).await.expect("state at branch B");
        let first = first_walk.resolve(&[set_a, set_b]).await.expect("first request resolve");
        assert_eq!(cache.counts(), (0, 1), "请求 1 首次决议必须是 MISS");

        // 请求 2：**新建** walk（请求边界），输入相同 ⇒ 必须命中请求 1 写入的条目。
        let mut second_walk = StateWalker::new(&storage, "12", &cache);
        let set_a = second_walk.state_at(&topic_a).await.expect("state at branch A");
        let set_b = second_walk.state_at(&topic_b).await.expect("state at branch B");
        let second = second_walk.resolve(&[set_a, set_b]).await.expect("second request resolve");
        assert_eq!(cache.counts(), (1, 1), "跨请求的同一冲突集合必须命中共享缓存（否则缓存仍被单次 walk 限制）");
        assert_eq!(first, second, "跨请求 HIT 必须返回与 MISS 相同的状态图");
    }

    /// A fork is resolved with v2.1: the **authorised** branch wins even though the
    /// rejected one is newer, and the read path then serves the record.
    ///
    /// `serial(state_ts_fallback)`：本用例断言"无记录 ⇒ 当前状态读必须失败"，
    /// 而 `txn_dedup_compensation_tests` 会临时设置 `SYNAPSE_ALLOW_STATE_TS_FALLBACK`
    /// （进程级环境变量）。两条用例必须互斥，否则开关期间本断言会被"推导回退"答出来。
    #[tokio::test]
    #[serial_test::serial(state_ts_fallback)]
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

        // Before the record exists the read path fails closed (A1), so the only
        // thing that can answer is the explicit derivation — which picks the
        // branch the auth rules reject. This is the behaviour the wiring replaces.
        assert!(
            storage.get_state_event(&room_id, "m.room.topic", "").await.is_err(),
            "no record ⇒ the current-state read must fail closed"
        );
        assert_eq!(
            derived_topic_id(&storage, &room_id).await,
            Some(topic_b.clone()),
            "no record ⇒ the derivation still prefers the newer sender clock"
        );

        assert_eq!(
            storage.get_forward_extremities_in_room(&room_id, 64).await.expect("extremities").len(),
            2,
            "the two topic branches are the forward extremities"
        );

        record(&storage, &room_storage, &state_groups, &ResolutionCache::new())
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
        let cache = ResolutionCache::new();
        let record = record(&storage, &room_storage, &state_groups, &cache);
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

    /// **A4**：没有分叉、也没有记录的房间，第一次写状态事件时**播种**出记录。
    ///
    /// 修复前这条必然红：`copy_forward` 在"无记录"分支直接 `Ok(())`，于是从未分叉的房间
    /// 永远没有 state group，读路径永远走 `DISTINCT ON … origin_server_ts DESC`。
    ///
    /// 用例故意让**第二条** topic 的 `origin_server_ts` **更旧**（DAG 上是后继）：
    /// - 时间戳推导会选第一条（`ts` 更新）；
    /// - 记录（copy-forward）会选第二条（DAG 顺序）；
    /// ⇒ 读路径取到哪一条，直接证明"记录已经接管、推导不再被读"。
    #[tokio::test]
    async fn first_state_write_seeds_a_record_for_an_unforked_room() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let storage = EventStorage::new(&pool, "example.com".to_string());
        let room_storage = RoomStorage { pool: pool.clone() };
        let state_groups = StateGroupStorage::new(&pool);
        let room_id = format!("!seed_{}:example.com", uuid::Uuid::new_v4());
        create_room(&room_storage, &room_id).await;

        let now = current_timestamp_millis();
        let create = "$seed_create:example.com";
        let alice_join = "$seed_alice_join:example.com";
        let topic_1 = "$seed_topic_1:example.com";

        storage
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
        storage
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
            .expect("alice join");
        storage
            .create_event_with_graph(
                state_params(
                    &room_id,
                    topic_1,
                    "@alice:example.com",
                    "m.room.topic",
                    "",
                    serde_json::json!({ "topic": "first" }),
                    now + 2,
                ),
                &[alice_join.to_string()],
                &[create.to_string(), alice_join.to_string()],
                3,
                None,
            )
            .await
            .expect("topic 1");

        assert!(
            state_groups.get_room_state_groups(&room_id, 1).await.expect("groups").is_empty(),
            "前提：这条房间路径没有分叉，写状态事件之前也没有任何记录"
        );
        assert_eq!(
            derived_topic_id(&storage, &room_id).await,
            Some(topic_1.to_string()),
            "播种前读路径没有记录可用（A1 失败关闭），推导给出的仍是这条 topic"
        );

        record(&storage, &room_storage, &state_groups, &ResolutionCache::new())
            .after_state_event(&room_id, topic_1, "m.room.topic", "")
            .await
            .expect("the first state write must seed the record");

        let groups = state_groups.get_room_state_groups(&room_id, 10).await.expect("groups");
        assert_eq!(groups.len(), 1, "首个状态写必须正好播种出一条记录，实得 {} 条", groups.len());
        assert_eq!(
            state_groups.get_state_group_for_event(topic_1).await.expect("binding"),
            Some(groups[0].id),
            "触发写入的事件必须绑定到播种出的记录"
        );
        let entries = state_groups.get_state_at_group(groups[0].id).await.expect("entries");
        let has = |ty: &str, key: &str, eid: &str| {
            entries.iter().any(|e| e.event_type == ty && e.state_key == key && e.event_id == eid)
        };
        assert!(has("m.room.create", "", create), "记录里必须有 create");
        assert!(has("m.room.member", "@alice:example.com", alice_join), "记录里必须有成员事件");
        assert!(has("m.room.topic", "", topic_1), "记录里必须有刚写入的 topic");

        // 第二条 topic：DAG 上是 topic_1 的后继，但 origin_server_ts **更旧**。
        let topic_2 = "$seed_topic_2:example.com";
        storage
            .create_event_with_graph(
                state_params(
                    &room_id,
                    topic_2,
                    "@alice:example.com",
                    "m.room.topic",
                    "",
                    serde_json::json!({ "topic": "second-but-older-timestamp" }),
                    now, // 比 topic_1 的 now+2 更旧
                ),
                &[topic_1.to_string()],
                &[create.to_string(), alice_join.to_string()],
                4,
                None,
            )
            .await
            .expect("topic 2");

        record(&storage, &room_storage, &state_groups, &ResolutionCache::new())
            .after_state_event(&room_id, topic_2, "m.room.topic", "")
            .await
            .expect("the second state write must fold forward");

        let groups = state_groups.get_room_state_groups(&room_id, 10).await.expect("groups");
        assert_eq!(groups.len(), 2, "第二次写状态事件应新增一条记录（不是重新播种）");
        assert_eq!(
            topic_id(&storage, &room_id).await,
            Some(topic_2.to_string()),
            "读路径必须返回 DAG 上更晚的那条 topic —— 时间戳更旧也一样。\
             若仍返回 `{}`，说明读路径还在走时间戳推导（A4 未生效）",
            topic_1
        );

        // A1 判据 ②（可变异自证）：同一次写入之后，推导**仍然**指向 topic_1，而记录
        // 指向 topic_2。两条路径给出相反答案 ⇒ 上面的断言确实来自记录，而不是
        // 因为推导恰好也选中了 topic_2。删掉记录读路径（或让读路径回落到推导），
        // 这条对照就会失败。
        assert_eq!(
            derived_topic_id(&storage, &room_id).await,
            Some(topic_1.to_string()),
            "推导必须仍按发送方时钟选 topic_1；记录与推导在此分歧，结论只能来自记录"
        );
    }
}

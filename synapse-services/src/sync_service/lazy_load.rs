use super::types::*;
use super::SyncService;
use crate::*;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// What a lazy-loading device has already been sent, per `(user, device, room)`.
///
/// Port of Synapse's `LazyLoadedMembersCache` (upstream #20289). The extra
/// bookkeeping is not optional — a membership may only be left out of a response
/// if the client **processed** the response that carried it, and user ids alone
/// cannot express that:
///
/// * **Position.** A client that processed a response comes back with a token at
///   or past that response's `next_batch`, so [`LazyLoadedMemberState::start_request`]
///   forgets every entry recorded *after* the request's `since`: those rode a
///   response the client never got (a request it timed out on and retried, or a
///   token it reloaded from an older persisted value). Without the position a
///   member dropped this way is lost for good.
/// * **Event id.** The entry remembers *which* membership event was sent, so a
///   membership that **changed** afterwards (new event id) is never suppressed —
///   the old `HashSet<String>` shape hid exactly those.
///
/// The notifier may build several responses for one long poll and return only the
/// first non-empty one; an empty response carries no members, so it records
/// nothing.
#[derive(Debug, Default, Clone)]
pub(crate) struct LazyLoadedMemberState {
    /// `member_user_id` → the membership we last handed over, and where.
    sent: HashMap<String, SentMembership>,
    /// Upper bound of the recorded positions, so [`Self::start_request`] can skip
    /// the scan in the steady state (`since` == the previous response's
    /// `next_batch`). Only ever moves forward: responses that started earlier may
    /// record *after* a later one did.
    newest: Option<i64>,
}

/// One entry of [`LazyLoadedMemberState`].
#[derive(Debug, Clone, PartialEq, Eq)]
struct SentMembership {
    /// Event id of the membership handed over, when this process knows it.
    ///
    /// `None` for an entry restored from storage, where only the position is
    /// kept: the position proves the client received *a* membership for that
    /// member, and whether it is still the current one is the changed-member
    /// set's job.
    event_id: Option<String>,
    /// Position of the `next_batch` of the response that carried it.
    sent_at: i64,
}

impl LazyLoadedMemberState {
    /// Forget the memberships this client's token proves it never received.
    ///
    /// `since == None` is an initial sync: a fresh client must be sent
    /// everything, so nothing may suppress — including what storage remembers.
    fn start_request(&mut self, since: Option<i64>) {
        let Some(since) = since else {
            self.sent.clear();
            self.newest = None;
            return;
        };

        // Steady state: `since` is the `next_batch` of the last response we
        // built, so nothing can have been recorded after it.
        if self.newest.is_none_or(|newest| newest <= since) {
            return;
        }

        self.sent.retain(|_, entry| entry.sent_at <= since);
        self.newest = Some(since);
    }

    /// Whether the client already holds membership `event_id` of `member`.
    ///
    /// An entry restored from storage carries a position but no event id; it
    /// suppresses, because the client provably received a membership for that
    /// member in a response it processed.
    fn was_sent(&self, member: &str, event_id: &str) -> bool {
        self.sent.get(member).is_some_and(|entry| entry.event_id.as_deref().is_none_or(|sent| sent == event_id))
    }

    /// Whether the client holds *any* membership event of `member`.
    ///
    /// The reconstruction form only asks that question: the block describes the
    /// state at the start of the window, so a member the client already knows is
    /// not replayed regardless of which event id the client saw — a change inside
    /// the window rides the timeline, and one outside it would be a lie.
    fn has_member(&self, member: &str) -> bool {
        self.sent.contains_key(member)
    }

    /// Record that membership `event_id` of `member` rides the response ending at
    /// position `at`.
    pub(crate) fn mark_sent(&mut self, member: &str, event_id: &str, at: i64) {
        self.sent.insert(member.to_string(), SentMembership { event_id: Some(event_id.to_string()), sent_at: at });
        self.advance_newest(at);
    }

    /// Fold in rows restored from storage, keeping only the ones this client's
    /// token proves it received.
    fn restore(&mut self, rows: Vec<synapse_storage::device::LazyLoadedMemberRow>, since: i64) {
        for row in rows {
            // `None` is "not recorded" (a row written before the column
            // existed). It cannot prove delivery, so it never suppresses; the
            // row heals the next time the member is sent.
            let Some(sent_at) = row.last_sent_position else {
                continue;
            };
            // A position newer than the client's token means the response that
            // carried it was never processed — that is the whole point of
            // persisting the position.
            if sent_at > since {
                continue;
            }
            self.sent.entry(row.member_user_id).or_insert(SentMembership { event_id: None, sent_at });
            self.advance_newest(sent_at);
        }
    }

    /// Keep `newest` an upper bound that only moves forward.
    fn advance_newest(&mut self, at: i64) {
        if self.newest.is_none_or(|newest| newest < at) {
            self.newest = Some(at);
        }
    }
}

impl SyncService {
    /// The lazy-load state for one room, restored from storage on a cold cache.
    ///
    /// Only an **incremental** request may trust storage: a request without a
    /// `since` token must send every membership, which is exactly what upstream's
    /// `start_request(None)` enforces for its in-memory cache.
    async fn lazy_loaded_member_state(
        &self,
        cache_key: &LazyLoadedMembersCacheKey,
        user_id: &str,
        device_id: Option<&str>,
        room_id: &str,
        since_position: Option<i64>,
    ) -> LazyLoadedMemberState {
        {
            let mut cache = self.lazy_loaded_members_cache.write().await;
            if let Some(state) = cache.get(cache_key) {
                // `.get()` on LruCache refreshes the LRU position (records as recently
                // used), so hot keys stay cached while cold ones are evicted on overflow.
                return state.clone();
            }
        }

        let mut restored = LazyLoadedMemberState::default();
        if let (Some(since), Some(device_id)) = (since_position, device_id) {
            match self.device_storage.get_lazy_loaded_members(user_id, device_id, room_id).await {
                Ok(rows) => restored.restore(rows, since),
                // A+ S-10: DB failure here degrades to "no known members", which makes the
                // next /sync re-send the full member list (correct but heavy). Surface it via
                // warn instead of swallowing silently.
                Err(e) => {
                    ::tracing::warn!(
                        error = %e,
                        user_id = %user_id,
                        room_id = %room_id,
                        "Failed to read lazy-loaded members; falling back to full state send"
                    );
                }
            }
        }

        let mut cache = self.lazy_loaded_members_cache.write().await;
        // Another request may have filled the key during the storage round trip;
        // its value wins so the two callers cannot disagree.
        if let Some(state) = cache.get(cache_key) {
            return state.clone();
        }
        cache.put(cache_key.clone(), restored.clone());
        restored
    }

    /// Whether the state block this response carries describes the state **at
    /// the start** of the returned timeline, i.e. the form the client re-aims by
    /// layering the returned timeline on top of it.
    ///
    /// A **limited classic `state`** block does. The client only sees the events
    /// inside the window, so a membership delta that happened *after* the window
    /// describes a state the shown events do not — replaying it would be a lie
    /// (that behaviour is deliberate and locked in by the
    /// `test_lazy_load_timeline_limited_*` cases).
    ///
    /// A **`state_after`** block (MSC4222) does not. It *is* the state at the end
    /// of the timeline and the client layers nothing onto it, so that same delta
    /// is exactly what the client is missing — and the next request's `since` has
    /// already moved past it, so dropping it loses the membership for good. That
    /// is upstream #20289's second defect in its worst form (`state_after` clients
    /// never see the change again).
    pub(crate) fn state_block_describes_the_timeline_start(timeline_limited: bool, state_after: bool) -> bool {
        timeline_limited && !state_after
    }

    /// See [`persist_lazy_loaded_members`].
    async fn persist_lazy_loaded_members(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        room_id: &str,
        sent_now: &[(String, String)],
        position: i64,
    ) {
        if sent_now.is_empty() {
            return;
        }

        if let Some(device_id) = device_id {
            // Every member of one response shares that response's position by
            // construction, which is what makes the stored row prunable later.
            let rows: Vec<synapse_storage::device::LazyLoadedMemberRow> = sent_now
                .iter()
                .map(|(member_user_id, _)| synapse_storage::device::LazyLoadedMemberRow {
                    member_user_id: member_user_id.clone(),
                    last_sent_position: Some(position),
                })
                .collect();
            let _ = self.device_storage.upsert_lazy_loaded_members(user_id, device_id, room_id, &rows).await;
        }
    }

    /// See [`apply_lazy_load_members`].
    pub(crate) async fn apply_lazy_load_members(&self, request: LazyLoadMembersRequest<'_>) -> Vec<Value> {
        let LazyLoadMembersRequest {
            state_events,
            timeline_events,
            user_id,
            device_id,
            room_id,
            room_filter,
            changed_member_ids,
            timeline_limited,
            state_after,
            enabled,
            response_position,
            since_position,
        } = request;
        if !enabled {
            return state_events;
        }

        let cache_key = LazyLoadedMembersCacheKey::new(user_id, device_id, room_id);
        let known = self.lazy_loaded_member_state(&cache_key, user_id, device_id, room_id, since_position).await;
        let include_redundant_members = Self::room_filter_requests_redundant_members(room_filter);
        // Only a classic `state` block has to describe the state at the *start* of
        // the returned timeline; that is the case which must not replay deltas
        // from outside the window (see the helper for why `state_after` differs).
        let state_at_timeline_start = Self::state_block_describes_the_timeline_start(timeline_limited, state_after);
        let changed_member_ids = changed_member_ids.filter(|_| !state_at_timeline_start).cloned().unwrap_or_default();
        let (filtered_events, sent_now) = Self::apply_lazy_load_members_with_cache(
            state_events,
            timeline_events,
            user_id,
            &known,
            include_redundant_members,
            &changed_member_ids,
            state_at_timeline_start,
        );

        // Commit under one lock, and only the memberships this response actually
        // hands over: recording one we suppressed would let a later response keep
        // suppressing it, which is how a changed membership got lost for good.
        {
            let mut cache = self.lazy_loaded_members_cache.write().await;
            let entry = cache.get_or_insert_mut(cache_key, || known.clone());
            entry.start_request(since_position);
            for (member, event_id) in &sent_now {
                entry.mark_sent(member, event_id, response_position);
            }
        }

        self.persist_lazy_loaded_members(user_id, device_id, room_id, &sent_now, response_position).await;

        filtered_events
    }

    /// See [`apply_lazy_load_members_with_cache`].
    ///
    /// Pure: it decides what to hand over and reports every membership it handed
    /// over as `(member_user_id, event_id)`, so the caller can record exactly
    /// those.
    ///
    /// `state_at_timeline_start` is the **derived** decision from
    /// [`SyncService::state_block_describes_the_timeline_start`], not the raw
    /// `limited` flag: the caller resolves "limited" × "which form the client
    /// reads" into one bit before any filtering happens, so this function cannot
    /// apply the reconstruction rule to a `state_after` block.
    pub(crate) fn apply_lazy_load_members_with_cache(
        state_events: Vec<Value>,
        timeline_events: &[RoomEvent],
        user_id: &str,
        known: &LazyLoadedMemberState,
        include_redundant_members: bool,
        changed_member_ids: &HashSet<String>,
        state_at_timeline_start: bool,
    ) -> (Vec<Value>, Vec<(String, String)>) {
        // Memberships riding the returned timeline are in the client's hands, so
        // they count as sent (upstream records `timeline_state` as well).
        let mut sent_now: Vec<(String, String)> = timeline_events
            .iter()
            .filter(|event| event.event_type == "m.room.member")
            .filter_map(|event| Some((event.state_key.clone()?, event.event_id.clone())))
            .collect();

        // Reconstruction form: the block must describe the state at the start of
        // the window, so only memberships the client does not already have are
        // handed over (no `required_members` gating, no changed-set replay).
        if state_at_timeline_start {
            let mut kept = Vec::with_capacity(state_events.len());
            for event in state_events {
                if event.get("type").and_then(|value| value.as_str()) != Some("m.room.member") {
                    kept.push(event);
                    continue;
                }
                let Some(state_key) = event.get("state_key").and_then(|value| value.as_str()) else {
                    continue;
                };
                let event_id = event.get("event_id").and_then(|value| value.as_str());
                // The client already holds a membership for this member ⇒ it is not
                // replayed here, whatever event id it saw: changes inside the window
                // ride the timeline, and changes outside it must not appear at all
                // (the block describes the window's *start*).
                let deliver = include_redundant_members || !known.has_member(state_key);
                if deliver {
                    if let Some(event_id) = event_id {
                        sent_now.push((state_key.to_string(), event_id.to_string()));
                    }
                    kept.push(event);
                }
            }
            return (kept, dedup_by_member(sent_now));
        }

        let mut required_members: HashSet<&str> = HashSet::from([user_id]);
        for event in timeline_events {
            required_members.insert(event.user_id.as_str());
            if event.event_type == "m.room.member" {
                if let Some(state_key) = event.state_key.as_deref() {
                    required_members.insert(state_key);
                }
            }
        }
        for user_id in changed_member_ids {
            required_members.insert(user_id.as_str());
        }

        let mut kept = Vec::with_capacity(state_events.len());
        for event in state_events {
            if event.get("type").and_then(|value| value.as_str()) != Some("m.room.member") {
                kept.push(event);
                continue;
            }

            let Some(state_key) = event.get("state_key").and_then(|value| value.as_str()) else {
                continue;
            };
            if !required_members.contains(state_key) {
                continue;
            }

            let event_id = event.get("event_id").and_then(|value| value.as_str());
            let deliver = include_redundant_members
                || changed_member_ids.contains(state_key)
                || event_id.is_none_or(|event_id| !known.was_sent(state_key, event_id));
            if deliver {
                if let Some(event_id) = event_id {
                    sent_now.push((state_key.to_string(), event_id.to_string()));
                }
                kept.push(event);
            }
        }

        (kept, dedup_by_member(sent_now))
    }
}

/// One `(member, event id)` per member, last occurrence winning.
///
/// A member can be handed over twice in one response — once as a timeline event
/// and once in the `state` block — and the persistence step feeds these rows to a
/// single `UNNEST … ON CONFLICT DO UPDATE`, which PostgreSQL **rejects** when the
/// same conflict row would be updated twice in one statement. Keeping the last
/// occurrence also means the state block's (current) membership wins over the
/// timeline's copy, which is the one worth remembering.
///
/// Sorted by member so the result does not depend on hash iteration order.
fn dedup_by_member(sent_now: Vec<(String, String)>) -> Vec<(String, String)> {
    let mut unique: HashMap<String, String> = HashMap::with_capacity(sent_now.len());
    for (member, event_id) in sent_now {
        unique.insert(member, event_id);
    }
    let mut deduped: Vec<(String, String)> = unique.into_iter().collect();
    deduped.sort();
    deduped
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_storage::device::LazyLoadedMemberRow;

    fn row(member: &str, position: Option<i64>) -> LazyLoadedMemberRow {
        LazyLoadedMemberRow { member_user_id: member.to_string(), last_sent_position: position }
    }

    #[test]
    fn a_client_that_processed_the_response_keeps_its_memberships() {
        let mut state = LazyLoadedMemberState::default();
        state.mark_sent("@bob:b", "$bob1", 10);

        // The client's token is the `next_batch` of that very response.
        state.start_request(Some(10));

        assert!(state.was_sent("@bob:b", "$bob1"), "the response was processed, so the member stays suppressed");
    }

    #[test]
    fn a_retried_or_rolled_back_request_re_sends_what_the_client_never_saw() {
        let mut state = LazyLoadedMemberState::default();
        state.mark_sent("@bob:b", "$bob1", 10);
        state.mark_sent("@carol:b", "$carol1", 20);

        // The client comes back with a token from *before* the second response:
        // it timed out on it, or reloaded from an older persisted token (#19978,
        // #20278). Nothing recorded after that token may suppress.
        state.start_request(Some(10));

        assert!(state.was_sent("@bob:b", "$bob1"), "recorded at the client's position ⇒ it did receive it");
        assert!(!state.was_sent("@carol:b", "$carol1"), "recorded after the client's position ⇒ it must be re-sent");
    }

    #[test]
    fn an_initial_sync_forgets_everything_including_restored_rows() {
        let mut state = LazyLoadedMemberState::default();
        state.restore(vec![row("@bob:b", Some(5))], 10);
        state.mark_sent("@carol:b", "$carol1", 10);
        assert!(state.was_sent("@bob:b", "$bob1"), "a restored row at/before `since` does suppress");

        state.start_request(None);

        assert!(!state.was_sent("@bob:b", "$bob1"), "a fresh client must be sent everything");
        assert!(!state.was_sent("@carol:b", "$carol1"));
    }

    #[test]
    fn a_changed_membership_is_never_suppressed() {
        let mut state = LazyLoadedMemberState::default();
        state.mark_sent("@bob:b", "$bob1", 10);

        assert!(state.was_sent("@bob:b", "$bob1"));
        assert!(
            !state.was_sent("@bob:b", "$bob2"),
            "a different event id means the membership changed; the client must see it"
        );
    }

    #[test]
    fn a_row_without_a_position_never_suppresses() {
        let mut state = LazyLoadedMemberState::default();
        state.restore(vec![row("@bob:b", None)], 10);

        assert!(
            !state.was_sent("@bob:b", "$bob1"),
            "an unrecorded position proves nothing, so the member must be re-sent"
        );
    }

    #[test]
    fn a_restored_row_newer_than_the_token_never_suppresses() {
        let mut state = LazyLoadedMemberState::default();
        state.restore(vec![row("@bob:b", Some(20)), row("@carol:b", Some(10))], 10);

        assert!(!state.was_sent("@bob:b", "$bob1"), "the response that carried it was never processed");
        assert!(state.was_sent("@carol:b", "$carol1"), "at the client's position ⇒ it did receive it");
    }

    #[test]
    fn a_restored_row_is_pruned_by_a_later_older_token() {
        let mut state = LazyLoadedMemberState::default();
        state.restore(vec![row("@bob:b", Some(5))], 10);
        assert!(state.was_sent("@bob:b", "$bob1"));

        // Same process, but now the client presents a token from before that row
        // (e.g. it reloaded from an older persisted token).
        state.start_request(Some(3));

        assert!(!state.was_sent("@bob:b", "$bob1"), "its position is newer than the client's token");
    }

    #[test]
    fn only_a_classic_state_block_is_read_as_the_timeline_start() {
        // Reconstruction form: a limited classic `state` block describes the state
        // at the *start* of the window, so deltas from after it must not be
        // replayed.
        assert!(SyncService::state_block_describes_the_timeline_start(true, false));

        // `state_after` is the state at the *end* of the timeline and nothing is
        // layered onto it: the same delta is exactly what the client is missing,
        // so it must be replayed. This is the case that used to be lost for good.
        assert!(
            !SyncService::state_block_describes_the_timeline_start(true, true),
            "a `state_after` block must replay membership changes from outside a limited window"
        );

        // Nothing was cut, so the window is the whole delta either way.
        assert!(!SyncService::state_block_describes_the_timeline_start(false, false));
        assert!(!SyncService::state_block_describes_the_timeline_start(false, true));
    }

    #[test]
    fn the_position_bound_only_moves_forward() {
        let mut state = LazyLoadedMemberState::default();
        // A response that started earlier may record after a later one did.
        state.mark_sent("@bob:b", "$bob1", 30);
        state.mark_sent("@carol:b", "$carol1", 10);

        // `newest` must still be the maximum, otherwise a token at 20 would prune
        // nothing and the entry recorded at 30 would keep suppressing.
        state.start_request(Some(20));

        assert!(!state.was_sent("@bob:b", "$bob1"));
        assert!(state.was_sent("@carol:b", "$carol1"));
    }
}

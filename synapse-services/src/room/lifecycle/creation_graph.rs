//! Graph metadata for the linear event sequence a room creation emits.
//!
//! # Why this is not the [`GraphMetadataWriter`] decorator
//!
//! Room creation writes its initial state inside one caller-managed
//! transaction. The decorator deliberately refuses to resolve graph metadata
//! there, because resolution reads *committed* state and the creation events are
//! not committed yet — resolving would produce an empty `auth_events` and
//! `prev_events: []` for events that are not DAG roots.
//!
//! Creation, however, knows its own graph exactly: the events are emitted in a
//! single linear order, each one's parent being the previous one. This tracker
//! turns that knowledge into the same [`EventGraphMetadata`] the decorator would
//! have computed, using the shared
//! [`select_auth_events`](crate::room::state::auth_events::select_auth_events)
//! selection rules so both paths agree on `auth_events`.
//!
//! # Ordering contract
//!
//! [`CreationGraph::next`] must be called **before** the event is written and
//! **once per event, in write order**. It selects `auth_events` from the state
//! that existed before the event (so an event can never authorise itself), then
//! records the event as the new DAG tip and as the occupant of its
//! `(type, state_key)` slot.
//!
//! # Bounded scope
//!
//! This covers the events a creation transaction emits. Later events in the
//! room's life go through the write-path decorator instead; if a room were ever
//! created with a fork (two events sharing a parent), the linear assumption here
//! would be wrong — creation does not fork, and a test pins that.

use serde_json::Value;

use crate::graph_metadata::EventGraphMetadata;
use crate::room::state::auth_events::{select_auth_events, AuthStateSnapshot};

/// Accumulates the DAG position of each event a creation transaction emits.
#[derive(Debug, Clone)]
pub(crate) struct CreationGraph {
    room_version: String,
    /// Depth of the most recently recorded event (0 before any).
    depth: i64,
    /// The DAG tip: the parent of the next event.
    tip: Option<String>,
    /// State as established by the events recorded so far.
    state: AuthStateSnapshot,
}

impl CreationGraph {
    /// Starts a graph in a room of `room_version`.
    pub(crate) fn new(room_version: &str) -> Self {
        Self { room_version: room_version.to_string(), depth: 0, tip: None, state: AuthStateSnapshot::default() }
    }

    /// Computes the graph fields for the next event and records it.
    ///
    /// `event_id` is the ID the caller is about to persist; `content` is the
    /// event content (used only for `auth_events` selection).
    pub(crate) fn next(
        &mut self,
        event_id: &str,
        event_type: &str,
        state_key: Option<&str>,
        sender: &str,
        content: &Value,
    ) -> EventGraphMetadata {
        let prev_events: Vec<String> = self.tip.iter().cloned().collect();
        let auth_events = select_auth_events(&self.room_version, &self.state, event_type, state_key, sender, content);
        let depth = self.depth + 1;

        if let Some(state_key) = state_key {
            self.state.insert(event_type, state_key, event_id);
        }
        self.tip = Some(event_id.to_string());
        self.depth = depth;

        EventGraphMetadata { room_version: self.room_version.clone(), prev_events, auth_events, depth }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ALICE: &str = "@alice:example.com";

    /// The exact sequence `create_room` emits, in order.
    fn linear_creation() -> Vec<EventGraphMetadata> {
        let mut graph = CreationGraph::new("11");
        let steps: Vec<(&str, &str, Option<&str>, Value)> = vec![
            ("$create", "m.room.create", Some(""), json!({"room_version": "11"})),
            ("$member", "m.room.member", Some(ALICE), json!({"membership": "join"})),
            ("$pl", "m.room.power_levels", Some(""), json!({"users": {ALICE: 100}})),
            ("$join_rules", "m.room.join_rules", Some(""), json!({"join_rule": "invite"})),
            ("$history", "m.room.history_visibility", Some(""), json!({"history_visibility": "shared"})),
            ("$name", "m.room.name", Some(""), json!({"name": "Room"})),
        ];
        steps
            .into_iter()
            .map(|(event_id, event_type, state_key, content)| {
                graph.next(event_id, event_type, state_key, ALICE, &content)
            })
            .collect()
    }

    #[test]
    fn create_event_is_the_root() {
        let graph = linear_creation();
        assert_eq!(graph[0], EventGraphMetadata::root("11"));
    }

    #[test]
    fn every_event_points_at_the_previous_one() {
        let graph = linear_creation();
        assert_eq!(graph[0].prev_events, Vec::<String>::new());
        assert_eq!(graph[1].prev_events, vec!["$create".to_string()]);
        assert_eq!(graph[2].prev_events, vec!["$member".to_string()]);
        assert_eq!(graph[5].prev_events, vec!["$history".to_string()]);
    }

    #[test]
    fn depth_increments_with_the_sequence() {
        let graph = linear_creation();
        let depths: Vec<i64> = graph.iter().map(|metadata| metadata.depth).collect();
        assert_eq!(depths, vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn auth_events_only_reference_state_that_already_exists() {
        let graph = linear_creation();
        // The join event can only auth against the create event: power_levels
        // and join_rules do not exist yet.
        assert_eq!(graph[1].auth_events, vec!["$create".to_string()]);
        // power_levels: create + the creator's member event.
        assert_eq!(graph[2].auth_events, vec!["$create".to_string(), "$member".to_string()]);
        // history_visibility: create + member + power_levels (join_rules is not
        // selected for a non-member event).
        assert_eq!(graph[4].auth_events, vec!["$create".to_string(), "$member".to_string(), "$pl".to_string()]);
    }

    #[test]
    fn an_event_never_authorises_itself() {
        let mut graph = CreationGraph::new("11");
        let metadata = graph.next("$member", "m.room.member", Some(ALICE), ALICE, &json!({"membership": "join"}));
        assert!(
            !metadata.auth_events.contains(&"$member".to_string()),
            "an event must not be its own auth event: {metadata:?}"
        );
    }

    #[test]
    fn invites_chain_onto_the_previous_event() {
        let mut graph = CreationGraph::new("11");
        graph.next("$create", "m.room.create", Some(""), ALICE, &json!({}));
        graph.next("$member", "m.room.member", Some(ALICE), ALICE, &json!({"membership": "join"}));
        let invite = graph.next(
            "$invite-bob",
            "m.room.member",
            Some("@bob:example.com"),
            ALICE,
            &json!({"membership": "invite"}),
        );
        assert_eq!(invite.prev_events, vec!["$member".to_string()]);
        assert_eq!(invite.depth, 3);
        // invite selects join_rules — absent at this point in the sequence — so
        // it is skipped rather than fabricated.
        assert_eq!(invite.auth_events, vec!["$create".to_string(), "$member".to_string()]);
    }
}

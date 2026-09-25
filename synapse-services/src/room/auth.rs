//! Auth event construction for v12+ room versions.
//!
//! Builds the `auth_events` list required for v12+ PDUs based on
//! the current room state. v12 requires the following auth events:
//! - m.room.create (always required)
//! - m.room.power_levels (if present)
//! - m.room.member (creator, always required)
//! - m.room.history_visibility (if present)

use std::collections::HashMap;

use synapse_storage::event::StateEvent;

/// Builds `auth_events` for v12+ event creation based on current room state.
///
/// # v12 Requirements (Matrix Spec)
///
/// Per MSC4311 and the Matrix v12 spec, every event in a v12 room must
/// include these auth_events:
///
/// 1. **m.room.create** — The room creation event (always present).
/// 2. **m.room.power_levels** — The power levels state event.
/// 3. **m.room.member** for the event creator — The creator's membership state.
/// 4. **m.room.history_visibility** — The history visibility state event.
///
/// # Usage
///
/// ```ignore
/// let state_events = event_reader.get_state_events(room_id).await?;
/// let builder = AuthEventBuilder::new(state_events);
/// let auth_events = builder.build_auth_events("m.room.message", None, "@user:server");
/// // auth_events = vec!["$create_evt", "$power_levels_evt", "$member_evt", "$history_visibility_evt"]
/// ```
pub struct AuthEventBuilder {
    /// State events indexed by (event_type, state_key).
    /// The empty string "" is used as state_key for state events
    /// without a specific key (e.g., m.room.create, m.room.power_levels).
    state_index: HashMap<(String, String), StateEvent>,
}

impl AuthEventBuilder {
    /// Create a new AuthEventBuilder from a list of state events.
    ///
    /// # Arguments
    ///
    /// * `state_events` - The current room state events (typically from `get_state_events`).
    pub fn new(state_events: Vec<StateEvent>) -> Self {
        let mut state_index = HashMap::new();

        for event in state_events {
            let event_type = event.event_type.clone().unwrap_or_default();
            let state_key = event.state_key.clone().unwrap_or_default();
            state_index.insert((event_type, state_key), event);
        }

        Self { state_index }
    }

    /// Build the auth_events list for a new event.
    ///
    /// # Arguments
    ///
    /// * `event_type` - The type of event being created (e.g., "m.room.message").
    /// * `state_key` - The state key for the event (if any).
    /// * `creator_user_id` - The user ID of the event creator (for m.room.member lookup).
    ///
    /// # Returns
    ///
    /// A vector of event IDs that should be in the auth_events field.
    ///
    /// # v12 Compliance
    ///
    /// This method returns exactly the auth_events required by the Matrix v12 spec:
    /// - m.room.create (required)
    /// - m.room.power_levels (required if present in state)
    /// - m.room.member for creator (required)
    /// - m.room.history_visibility (required if present in state)
    pub fn build_auth_events(&self, _event_type: &str, _state_key: Option<&str>, creator_user_id: &str) -> Vec<String> {
        let mut auth_events = Vec::new();

        // 1. m.room.create (always required for v12)
        if let Some(create_event) = self.state_index.get(&("m.room.create".to_string(), "".to_string())) {
            auth_events.push(create_event.event_id.clone());
        }

        // 2. m.room.power_levels (required if present)
        if let Some(pl_event) = self.state_index.get(&("m.room.power_levels".to_string(), "".to_string())) {
            auth_events.push(pl_event.event_id.clone());
        }

        // 3. m.room.member for creator (always required).
        //
        // `creator_user_id` is a *full* user ID (`@user:server`) — see the
        // module-level usage example and this module's tests — and the state key
        // of an `m.room.member` event **is** that user ID.  The previous
        // `format!("@{creator_user_id}")` therefore looked up
        // `@@user:server`, matched nothing, and silently dropped the creator's
        // membership auth event (observed as 4→3 and 2→1 events in the tests
        // below).
        if let Some(member_event) = self.state_index.get(&("m.room.member".to_string(), creator_user_id.to_string())) {
            auth_events.push(member_event.event_id.clone());
        }

        // 4. m.room.history_visibility (required if present)
        if let Some(hv_event) = self.state_index.get(&("m.room.history_visibility".to_string(), "".to_string())) {
            auth_events.push(hv_event.event_id.clone());
        }

        auth_events
    }

    /// Get a specific state event by type and state_key.
    pub fn get_state_event(&self, event_type: &str, state_key: &str) -> Option<&StateEvent> {
        self.state_index.get(&(event_type.to_string(), state_key.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_state_event(event_id: &str, event_type: &str, state_key: &str) -> StateEvent {
        StateEvent {
            event_id: event_id.to_string(),
            room_id: "!test:example.com".to_string(),
            sender: "@user:example.com".to_string(),
            event_type: Some(event_type.to_string()),
            content: json!({}),
            state_key: Some(state_key.to_string()),
            unsigned: None,
            is_redacted: None,
            origin_server_ts: 0,
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

    #[test]
    fn test_build_auth_events_with_all_required() {
        let state_events = vec![
            make_state_event("$create:example.com", "m.room.create", ""),
            make_state_event("$power:example.com", "m.room.power_levels", ""),
            make_state_event("$member:example.com", "m.room.member", "@creator:example.com"),
            make_state_event("$history:example.com", "m.room.history_visibility", ""),
        ];

        let builder = AuthEventBuilder::new(state_events);
        let auth_events = builder.build_auth_events("m.room.message", None, "@creator:example.com");

        assert_eq!(auth_events.len(), 4);
        assert!(auth_events.contains(&"$create:example.com".to_string()));
        assert!(auth_events.contains(&"$power:example.com".to_string()));
        assert!(auth_events.contains(&"$member:example.com".to_string()));
        assert!(auth_events.contains(&"$history:example.com".to_string()));
    }

    #[test]
    fn test_build_auth_events_with_partial_state() {
        // Only m.room.create and m.room.member (no power_levels or history_visibility)
        let state_events = vec![
            make_state_event("$create:example.com", "m.room.create", ""),
            make_state_event("$member:example.com", "m.room.member", "@creator:example.com"),
        ];

        let builder = AuthEventBuilder::new(state_events);
        let auth_events = builder.build_auth_events("m.room.message", None, "@creator:example.com");

        assert_eq!(auth_events.len(), 2);
        assert!(auth_events.contains(&"$create:example.com".to_string()));
        assert!(auth_events.contains(&"$member:example.com".to_string()));
    }

    #[test]
    fn test_build_auth_events_empty_state() {
        let state_events = vec![];

        let builder = AuthEventBuilder::new(state_events);
        let auth_events = builder.build_auth_events("m.room.message", None, "@creator:example.com");

        // Only m.room.create is present in the state index
        assert_eq!(auth_events.len(), 0);
    }

    #[test]
    fn test_get_state_event() {
        let state_events = vec![make_state_event("$create:example.com", "m.room.create", "")];

        let builder = AuthEventBuilder::new(state_events);
        let event = builder.get_state_event("m.room.create", "");

        assert!(event.is_some());
        assert_eq!(event.unwrap().event_id, "$create:example.com");
    }
}

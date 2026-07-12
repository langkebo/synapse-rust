//! Shared response-assembly helpers used by both `/sync` and sliding sync.
//!
//! Event-to-JSON conversion is identical across both code paths (Client format).
//! Federation-format variants live in `sync_service::response` where the extra
//! depth/origin fields are needed.

use serde_json::{json, Value};
use synapse_storage::event::RoomEvent;
use synapse_storage::StateEvent;

/// Convert a [`RoomEvent`] to its Client-format JSON representation.
pub fn room_event_to_json(event: &RoomEvent) -> Value {
    let now = chrono::Utc::now().timestamp_millis();
    let age = now.saturating_sub(event.origin_server_ts);
    let mut obj = json!({
        "type": event.event_type,
        "content": event.content,
        "sender": event.user_id,
        "origin_server_ts": event.origin_server_ts,
        "event_id": event.event_id,
        "room_id": event.room_id,
        "unsigned": {
            "age": age
        }
    });
    if let Some(ref state_key) = event.state_key {
        obj["state_key"] = json!(state_key);
    }
    obj
}

/// Convert a [`StateEvent`] to its Client-format JSON representation.
pub fn state_event_to_json(event: &StateEvent) -> Value {
    let now = chrono::Utc::now().timestamp_millis();
    let age = now.saturating_sub(event.origin_server_ts);
    let sender = event.user_id.as_deref().unwrap_or(&event.sender);
    let event_type = event.event_type.as_deref().unwrap_or("m.room.message");
    let mut obj = json!({
        "type": event_type,
        "content": event.content,
        "sender": sender,
        "origin_server_ts": event.origin_server_ts,
        "event_id": event.event_id,
        "room_id": event.room_id,
        "unsigned": {
            "age": age
        }
    });
    if let Some(ref state_key) = event.state_key {
        obj["state_key"] = json!(state_key);
    }
    obj
}

//! Event → JSON conversion helpers (single, batch, with/without `age` field).

use crate::event_models::RoomEvent;
use serde_json::{json, Value};

/// The `(hashes, signatures)` pair when **both** halves are present and usable.
///
/// Both must be objects and non-empty: stored hashes paired with a fresh (or
/// absent) signature would describe two different byte sequences, so partial
/// material is rejected here rather than persisted as if it were a verifiable
/// pair. `sha256` must be a non-empty string, since that is the only content
/// hash the repo verifies.
///
/// Single implementation shared by the inbound PDU projector
/// (`synapse-web/src/routes/federation/pdu.rs`, which reads it off a stored row)
/// and the inbound persistence path
/// (`synapse-services/src/room/federation_broadcast.rs`, which reads it off a
/// received PDU) — the two must agree on "usable", or an event could be stored
/// as verifiable and then refused at projection time.
pub fn signature_material(hashes: Option<&Value>, signatures: Option<&Value>) -> Option<(Value, Value)> {
    let hashes = hashes.filter(|value| value.is_object())?;
    if hashes.get("sha256").and_then(Value::as_str).is_none_or(str::is_empty) {
        return None;
    }
    let signatures = signatures.filter(|value| value.is_object())?;
    if signatures.as_object().is_none_or(|map| map.is_empty()) {
        return None;
    }
    Some((hashes.clone(), signatures.clone()))
}

/// Read a JSONB column that must be an array of event IDs.
///
/// Returns `None` for `NULL`, for a non-array, and for an array containing a
/// non-string element — all three mean "not usable as PDU graph metadata".
///
/// Single implementation shared by the federation PDU projector
/// (`synapse-web/src/routes/federation/pdu.rs`) and the outbound broadcast path
/// (`synapse-services/src/room/federation_broadcast.rs`): both must agree on
/// what counts as usable `prev_events` / `auth_events`, or an event could be
/// signed outbound and refused inbound (or vice versa).
pub fn event_id_array(value: Option<&Value>) -> Option<Vec<String>> {
    let array = value?.as_array()?;
    let mut ids = Vec::with_capacity(array.len());
    for element in array {
        ids.push(element.as_str()?.to_string());
    }
    Some(ids)
}

/// Events the to.
pub fn event_to_json(event: &RoomEvent) -> Value {
    let age = crate::time::calculate_age(event.origin_server_ts);

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

/// Eventss the to.
pub fn events_to_json(events: &[RoomEvent]) -> Vec<Value> {
    events.iter().map(event_to_json).collect()
}

/// Events the to.
pub fn event_to_json_without_age(event: &RoomEvent) -> Value {
    let mut obj = json!({
        "type": event.event_type,
        "content": event.content,
        "sender": event.user_id,
        "origin_server_ts": event.origin_server_ts,
        "event_id": event.event_id,
        "room_id": event.room_id,
    });

    if let Some(ref state_key) = event.state_key {
        obj["state_key"] = json!(state_key);
    }

    obj
}

/// Eventss the to.
pub fn events_to_json_without_age(events: &[RoomEvent]) -> Vec<Value> {
    events.iter().map(event_to_json_without_age).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn signature_material_requires_both_usable_halves() {
        let (hashes, signatures) = signature_material(
            Some(&json!({"sha256": "abc"})),
            Some(&json!({"origin.example.com": {"ed25519:1": "sig"}})),
        )
        .expect("usable pair");
        assert_eq!(hashes["sha256"], json!("abc"));
        assert!(signatures.get("origin.example.com").is_some());

        for (hashes, signatures) in [
            (Some(json!({"sha256": "abc"})), None),
            (None, Some(json!({"o": {"ed25519:1": "s"}}))),
            (Some(json!({"sha256": ""})), Some(json!({"o": {"ed25519:1": "s"}}))),
            (Some(json!({"sha512": "abc"})), Some(json!({"o": {"ed25519:1": "s"}}))),
            (Some(json!({"sha256": "abc"})), Some(json!({}))),
            (Some(json!("abc")), Some(json!({"o": {"ed25519:1": "s"}}))),
        ] {
            assert!(
                signature_material(hashes.as_ref(), signatures.as_ref()).is_none(),
                "partial material must be refused: {hashes:?} / {signatures:?}"
            );
        }
    }

    use super::*;
    use crate::current_timestamp_millis;
    use serde_json::json;

    fn create_test_event() -> RoomEvent {
        RoomEvent {
            event_id: "test_event_id".to_string(),
            room_id: "!test:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content: json!({"body": "test message"}),
            origin_server_ts: current_timestamp_millis() - 1000,
            user_id: "@test:example.com".to_string(),
            state_key: None,
            depth: 0,
            processed_ts: 0,
            not_before: 0,
            status: None,
            origin: "example.com".to_string(),
            stream_ordering: Some(1),
        }
    }

    #[test]
    fn test_event_to_json() {
        let event = create_test_event();
        let json = event_to_json(&event);

        assert_eq!(json["type"], "m.room.message");
        assert_eq!(json["event_id"], "test_event_id");
        assert_eq!(json["room_id"], "!test:example.com");
        assert_eq!(json["sender"], "@test:example.com");
        assert!(json.get("unsigned").is_some());
        assert!(json["unsigned"]["age"].as_i64().unwrap() >= 1000);
    }

    #[test]
    fn test_event_to_json_with_state_key() {
        let mut event = create_test_event();
        event.state_key = Some("".to_string());
        event.event_type = "m.room.member".to_string();

        let json = event_to_json(&event);
        assert!(json.get("state_key").is_some());
    }

    #[test]
    fn test_events_to_json() {
        let events = vec![create_test_event(), create_test_event()];
        let json_events = events_to_json(&events);

        assert_eq!(json_events.len(), 2);
    }

    #[test]
    fn test_event_to_json_without_age() {
        let event = create_test_event();
        let json = event_to_json_without_age(&event);

        assert_eq!(json["type"], "m.room.message");
        assert!(json.get("unsigned").is_none());
    }
}

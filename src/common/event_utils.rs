use crate::storage::RoomEvent;
use serde_json::{json, Value};

pub fn event_to_json(event: &RoomEvent) -> Value {
    let age = crate::common::time::calculate_age(event.origin_server_ts);

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

pub fn events_to_json(events: &[RoomEvent]) -> Vec<Value> {
    events.iter().map(event_to_json).collect()
}

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

pub fn events_to_json_without_age(events: &[RoomEvent]) -> Vec<Value> {
    events.iter().map(event_to_json_without_age).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_event() -> RoomEvent {
        RoomEvent {
            event_id: "test_event_id".to_string(),
            room_id: "!test:example.com".to_string(),
            event_type: "m.room.message".to_string(),
            content: json!({"body": "test message"}),
            origin_server_ts: chrono::Utc::now().timestamp_millis() - 1000,
            user_id: "@test:example.com".to_string(),
            state_key: None,
            depth: 0,
            processed_ts: 0,
            not_before: 0,
            status: None,
            reference_image: None,
            origin: "example.com".to_string(),
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

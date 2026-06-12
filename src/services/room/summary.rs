pub use synapse_services::room::summary::*;

#[cfg(test)]
mod tests {
    use super::{CreateRoomSummaryRequest, RoomSummaryResponse};

    #[test]
    fn root_room_summary_service_reexport_keeps_create_request_shape() {
        let request = CreateRoomSummaryRequest {
            room_id: "!room:example.com".to_string(),
            room_type: Some("m.space".to_string()),
            name: Some("Test Room".to_string()),
            topic: Some("A test room".to_string()),
            avatar_url: Some("mxc://avatar".to_string()),
            canonical_alias: Some("#test:example.com".to_string()),
            join_rule: Some("public".to_string()),
            history_visibility: Some("shared".to_string()),
            guest_access: Some("forbidden".to_string()),
            is_direct: Some(false),
            is_space: Some(true),
        };

        assert_eq!(request.room_id, "!room:example.com");
        assert_eq!(request.room_type.as_deref(), Some("m.space"));
        assert_eq!(request.is_space, Some(true));
    }

    #[test]
    fn root_room_summary_service_reexport_keeps_response_serialization() {
        let response = RoomSummaryResponse {
            room_id: "!room:example.com".to_string(),
            room_type: None,
            name: Some("Test Room".to_string()),
            topic: Some("A test room".to_string()),
            avatar_url: None,
            canonical_alias: None,
            join_rule: "public".to_string(),
            history_visibility: "shared".to_string(),
            guest_access: "forbidden".to_string(),
            is_direct: false,
            is_space: false,
            is_encrypted: true,
            member_count: 10,
            joined_member_count: 8,
            invited_member_count: 2,
            heroes: Vec::new(),
            last_event_ts: Some(1234567890),
            last_message_ts: Some(1234567891),
        };

        let json = serde_json::to_value(&response).expect("serialize room summary response");
        assert_eq!(json.get("room_id").and_then(serde_json::Value::as_str), Some("!room:example.com"));
        assert_eq!(json.get("is_encrypted").and_then(serde_json::Value::as_bool), Some(true));
    }
}

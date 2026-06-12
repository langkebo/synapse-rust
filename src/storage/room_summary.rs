pub use synapse_storage::room_summary::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_room_summary_storage_reexport_keeps_response_shape() {
        let response = RoomSummaryResponse {
            room_id: "!room:example.com".to_string(),
            room_type: Some("m.space".to_string()),
            name: Some("Test Room".to_string()),
            topic: Some("A test room".to_string()),
            avatar_url: Some("mxc://avatar".to_string()),
            canonical_alias: Some("#test:example.com".to_string()),
            join_rule: "public".to_string(),
            history_visibility: "shared".to_string(),
            guest_access: "forbidden".to_string(),
            is_direct: false,
            is_space: true,
            is_encrypted: true,
            member_count: 10,
            joined_member_count: 8,
            invited_member_count: 2,
            heroes: vec![RoomSummaryHero {
                user_id: "@alice:example.com".to_string(),
                display_name: Some("Alice".to_string()),
                avatar_url: None,
            }],
            last_event_ts: Some(1234567890),
            last_message_ts: Some(1234567891),
        };

        let json = serde_json::to_value(&response).expect("serialize room summary response");
        assert_eq!(json.get("room_id").and_then(serde_json::Value::as_str), Some("!room:example.com"));
        assert_eq!(json.get("is_space").and_then(serde_json::Value::as_bool), Some(true));
    }

    #[test]
    fn root_room_summary_storage_reexport_keeps_request_defaults() {
        let request = UpdateRoomSummaryRequest {
            name: Some("Updated Name".to_string()),
            topic: None,
            avatar_url: Some("mxc://new".to_string()),
            canonical_alias: None,
            join_rule: None,
            history_visibility: None,
            guest_access: None,
            is_direct: None,
            is_space: None,
            is_encrypted: Some(true),
            last_event_id: Some("$event".to_string()),
            last_event_ts: Some(1234567890),
            last_message_ts: Some(1234567891),
            hero_users: Some(serde_json::json!(["@alice:example.com"])),
        };

        assert_eq!(request.name.as_deref(), Some("Updated Name"));
        assert_eq!(request.is_encrypted, Some(true));
        assert_eq!(request.last_event_id.as_deref(), Some("$event"));
    }

    #[test]
    fn test_room_summary_creation() {
        let summary = RoomSummary {
            id: Some(1),
            room_id: "!room:example.com".to_string(),
            room_type: None,
            name: Some("Test Room".to_string()),
            topic: Some("A test room".to_string()),
            avatar_url: Some("mxc://avatar".to_string()),
            canonical_alias: None,
            join_rule: "public".to_string(),
            history_visibility: "shared".to_string(),
            guest_access: "forbidden".to_string(),
            is_direct: false,
            is_space: false,
            is_encrypted: false,
            member_count: 10,
            joined_member_count: 8,
            invited_member_count: 2,
            hero_users: serde_json::json!([]),
            last_event_id: None,
            last_event_ts: None,
            last_message_ts: None,
            unread_notifications: 0,
            unread_highlight: 0,
            updated_ts: Some(1234567890),
            created_ts: Some(1234567800),
        };
        assert_eq!(summary.room_id, "!room:example.com");
        assert!(summary.name.is_some());
    }

    #[test]
    fn test_room_summary_member_creation() {
        let member = RoomSummaryMember {
            id: 1,
            room_id: "!room:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            display_name: Some("Alice".to_string()),
            avatar_url: Some("mxc://alice".to_string()),
            membership: "join".to_string(),
            is_hero: true,
            last_active_ts: Some(1234567890),
            updated_ts: 1234567890,
            created_ts: 1234567800,
        };
        assert_eq!(member.user_id, "@alice:example.com");
        assert!(member.is_hero);
    }

    #[test]
    fn test_room_summary_state_creation() {
        let state = RoomSummaryState {
            id: 1,
            room_id: "!room:example.com".to_string(),
            event_type: "m.room.create".to_string(),
            state_key: "".to_string(),
            event_id: None,
            content: serde_json::json!({"creator": "@admin:example.com"}),
            updated_ts: 1234567890,
        };
        assert_eq!(state.room_id, "!room:example.com");
    }

    #[test]
    fn test_room_summary_stats_creation() {
        let stats = RoomSummaryStats {
            id: 1,
            room_id: "!room:example.com".to_string(),
            total_events: 100,
            total_state_events: 20,
            total_messages: 100,
            total_media: 10,
            storage_size: 1048576,
            last_updated_ts: 1234567890,
        };
        assert_eq!(stats.total_messages, 100);
    }

    #[test]
    fn test_create_room_summary_request() {
        let request = CreateRoomSummaryRequest {
            room_id: "!room:example.com".to_string(),
            room_type: None,
            name: Some("New Room".to_string()),
            topic: Some("Topic".to_string()),
            avatar_url: None,
            canonical_alias: None,
            join_rule: Some("public".to_string()),
            history_visibility: Some("shared".to_string()),
            guest_access: Some("forbidden".to_string()),
            is_direct: Some(false),
            is_space: Some(false),
        };
        assert_eq!(request.room_id, "!room:example.com");
    }

    #[test]
    fn test_update_room_summary_request() {
        let request = UpdateRoomSummaryRequest {
            name: Some("Updated Name".to_string()),
            topic: None,
            avatar_url: Some("mxc://new".to_string()),
            canonical_alias: None,
            join_rule: None,
            history_visibility: None,
            guest_access: None,
            is_direct: None,
            is_space: None,
            is_encrypted: None,
            last_event_id: None,
            last_event_ts: None,
            last_message_ts: None,
            hero_users: None,
        };
        assert!(request.name.is_some());
    }

    #[test]
    fn test_hero_users_json() {
        let heroes = vec![
            RoomSummaryHero {
                user_id: "@alice:example.com".to_string(),
                display_name: Some("Alice".to_string()),
                avatar_url: None,
            },
            RoomSummaryHero {
                user_id: "@bob:example.com".to_string(),
                display_name: Some("Bob".to_string()),
                avatar_url: Some("mxc://bob".to_string()),
            },
        ];
        let json = serde_json::to_string(&heroes).unwrap();
        assert!(json.contains("@alice:example.com"));
    }

    #[test]
    fn test_room_summary_optional_fields() {
        let summary = RoomSummary {
            id: Some(2),
            room_id: "!room2:example.com".to_string(),
            room_type: None,
            name: None,
            topic: None,
            avatar_url: None,
            canonical_alias: None,
            join_rule: "public".to_string(),
            history_visibility: "shared".to_string(),
            guest_access: "forbidden".to_string(),
            is_direct: false,
            is_space: false,
            is_encrypted: false,
            member_count: 0,
            joined_member_count: 0,
            invited_member_count: 0,
            hero_users: serde_json::json!([]),
            last_event_id: None,
            last_event_ts: None,
            last_message_ts: None,
            unread_notifications: 0,
            unread_highlight: 0,
            updated_ts: Some(0),
            created_ts: Some(0),
        };
        assert!(summary.name.is_none());
        assert!(summary.topic.is_none());
    }
}

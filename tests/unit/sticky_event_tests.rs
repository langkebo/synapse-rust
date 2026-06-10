#![cfg(test)]

mod sticky_event_suite {
    use synapse_rust::storage::sticky_event::StickyEvent;

    #[test]
    fn test_sticky_event_struct() {
        let event = StickyEvent {
            room_id: "!room:localhost".to_string(),
            user_id: "@user:localhost".to_string(),
            event_id: "$event:localhost".to_string(),
            event_type: "m.room.message".to_string(),
            sticky: true,
            updated_at: 1700000000000i64,
        };

        assert_eq!(event.room_id, "!room:localhost");
        assert_eq!(event.user_id, "@user:localhost");
        assert_eq!(event.event_id, "$event:localhost");
        assert_eq!(event.event_type, "m.room.message");
        assert!(event.sticky);
    }

    #[test]
    fn test_event_type_validation() {
        let valid_types =
            vec!["m.room.message", "m.room.topic", "m.room.avatar", "m.room.name", "m.room.member", "m.room.encrypted"];

        for event_type in valid_types {
            assert!(
                event_type.starts_with("m.") || event_type.starts_with("com."),
                "Event type should start with m. or com."
            );
        }
    }
}
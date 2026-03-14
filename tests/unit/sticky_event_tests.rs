#![cfg(test)]

mod sticky_event_tests {
    use synapse_rust::storage::sticky_event::{StickyEvent, StickyEventStorage};
    use sqlx::postgres::PgPoolOptions;
    use std::sync::Arc;
    use std::time::Duration;

    /// Integration test for sticky event storage
    /// Run with: TEST_DATABASE_URL=postgresql://user:pass@localhost/db cargo test sticky_event_tests --test unit
    #[tokio::test]
    #[ignore] // Requires database setup
    async fn test_sticky_event_operations() {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://synapse:secret@localhost:5432/synapse_test".to_string());

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        let storage = StickyEventStorage::new(Arc::new(pool));
        let room_id = "!testroom:localhost";
        let user_id = "@testuser:localhost";
        let event_id = "$testevent:localhost";
        let event_type = "m.room.message";

        // Set sticky event
        storage.set_sticky_event(room_id, user_id, event_id, event_type, true)
            .await
            .expect("Failed to set sticky event");

        // Get sticky event
        let result = storage.get_sticky_event(room_id, user_id, event_type)
            .await
            .expect("Failed to get sticky event");

        assert!(result.is_some());
        let sticky = result.unwrap();
        assert_eq!(sticky.room_id, room_id);
        assert_eq!(sticky.user_id, user_id);
        assert_eq!(sticky.event_id, event_id);
        assert_eq!(sticky.event_type, event_type);
        assert!(sticky.sticky);

        // Get all sticky events
        let all_events = storage.get_all_sticky_events(room_id, user_id)
            .await
            .expect("Failed to get all sticky events");

        assert!(!all_events.is_empty());
        assert!(all_events.iter().any(|e| e.event_type == event_type));

        // Clear sticky event
        storage.clear_sticky_event(room_id, user_id, event_type)
            .await
            .expect("Failed to clear sticky event");

        // Verify cleared
        let result = storage.get_sticky_event(room_id, user_id, event_type)
            .await
            .expect("Failed to get sticky event");

        assert!(result.is_none() || !result.unwrap().sticky);
    }

    #[tokio::test]
    #[ignore] // Requires database setup
    async fn test_multiple_sticky_events() {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://synapse:secret@localhost:5432/synapse_test".to_string());

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        let storage = StickyEventStorage::new(Arc::new(pool));
        let room_id = "!testroom:localhost";
        let user_id = "@testuser:localhost";

        // Set multiple sticky events
        storage.set_sticky_event(room_id, user_id, "$event1:localhost", "m.room.message", true)
            .await
            .expect("Failed to set sticky event 1");
        
        storage.set_sticky_event(room_id, user_id, "$event2:localhost", "m.room.topic", true)
            .await
            .expect("Failed to set sticky event 2");
        
        storage.set_sticky_event(room_id, user_id, "$event3:localhost", "m.room.avatar", true)
            .await
            .expect("Failed to set sticky event 3");

        // Get all sticky events
        let all_events = storage.get_all_sticky_events(room_id, user_id)
            .await
            .expect("Failed to get all sticky events");

        assert_eq!(all_events.len(), 3);

        // Verify each event type
        let event_types: Vec<&str> = all_events.iter().map(|e| e.event_type.as_str()).collect();
        assert!(event_types.contains(&"m.room.message"));
        assert!(event_types.contains(&"m.room.topic"));
        assert!(event_types.contains(&"m.room.avatar"));
    }

    #[tokio::test]
    #[ignore] // Requires database setup
    async fn test_get_rooms_with_sticky_events() {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://synapse:secret@localhost:5432/synapse_test".to_string());

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        let storage = StickyEventStorage::new(Arc::new(pool));
        let user_id = "@testuser:localhost";

        // Set sticky events in different rooms
        storage.set_sticky_event("!room1:localhost", user_id, "$event1:localhost", "m.room.message", true)
            .await
            .expect("Failed to set sticky event");
        
        storage.set_sticky_event("!room2:localhost", user_id, "$event2:localhost", "m.room.message", true)
            .await
            .expect("Failed to set sticky event");

        // Get rooms with sticky events
        let rooms = storage.get_rooms_with_sticky_events(user_id)
            .await
            .expect("Failed to get rooms with sticky events");

        assert!(rooms.len() >= 2);
        assert!(rooms.contains(&"!room1:localhost".to_string()));
        assert!(rooms.contains(&"!room2:localhost".to_string()));
    }

    #[tokio::test]
    #[ignore] // Requires database setup
    async fn test_update_sticky_event() {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://synapse:secret@localhost:5432/synapse_test".to_string());

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        let storage = StickyEventStorage::new(Arc::new(pool));
        let room_id = "!testroom:localhost";
        let user_id = "@testuser:localhost";
        let event_type = "m.room.message";

        // Set initial sticky event
        storage.set_sticky_event(room_id, user_id, "$event1:localhost", event_type, true)
            .await
            .expect("Failed to set initial sticky event");

        // Update to new event
        storage.set_sticky_event(room_id, user_id, "$event2:localhost", event_type, true)
            .await
            .expect("Failed to update sticky event");

        // Verify updated
        let result = storage.get_sticky_event(room_id, user_id, event_type)
            .await
            .expect("Failed to get sticky event");

        assert!(result.is_some());
        assert_eq!(result.unwrap().event_id, "$event2:localhost");
    }

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
        // Test valid Matrix event types
        let valid_types = vec![
            "m.room.message",
            "m.room.topic",
            "m.room.avatar",
            "m.room.name",
            "m.room.member",
            "m.room.encrypted",
        ];

        for event_type in valid_types {
            assert!(event_type.starts_with("m.") || event_type.starts_with("com."), 
                "Event type should start with m. or com.");
        }
    }
}

#![cfg(test)]

mod sticky_event_integration_suite {
    use sqlx::PgPool;
    use synapse_storage::sticky_event::{StickyEvent, StickyEventStorage};

    /// Insert prerequisite rooms and users rows so FK constraints on
    /// room_sticky_events are satisfied. Uses ON CONFLICT DO NOTHING so
    /// repeated calls within the same schema are safe.
    async fn ensure_room_and_user(pool: &PgPool, room_id: &str, user_id: &str) {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("INSERT INTO rooms (room_id, room_version, creator, created_ts) VALUES ($1, '11', $2, $3) ON CONFLICT (room_id) DO NOTHING")
            .bind(room_id)
            .bind(user_id)
            .bind(now)
            .execute(pool)
            .await
            .expect("Failed to ensure room row");
        sqlx::query("INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING")
            .bind(user_id)
            .bind(user_id.trim_start_matches('@'))
            .bind(now)
            .execute(pool)
            .await
            .expect("Failed to ensure user row");
    }

    #[tokio::test]
    async fn test_sticky_event_operations() {
        let pool = crate::require_test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let room_id = "!testroom:localhost";
        let user_id = "@testuser:localhost";
        let event_id = "$testevent:localhost";
        let event_type = "m.room.message";

        ensure_room_and_user(&pool, room_id, user_id).await;

        storage
            .set_is_sticky_event(room_id, user_id, event_id, event_type, true)
            .await
            .expect("Failed to set sticky event");

        let result =
            storage.get_is_sticky_event(room_id, user_id, event_type).await.expect("Failed to get sticky event");

        assert!(result.is_some());
        let sticky = result.unwrap();
        assert_eq!(sticky.room_id, room_id);
        assert_eq!(sticky.user_id, user_id);
        assert_eq!(sticky.event_id, event_id);
        assert_eq!(sticky.event_type, event_type);
        assert!(sticky.is_sticky);

        let all_events =
            storage.get_all_is_sticky_events(room_id, user_id).await.expect("Failed to get all sticky events");

        assert!(!all_events.is_empty());
        assert!(all_events.iter().any(|e| e.event_type == event_type));

        storage.clear_is_sticky_event(room_id, user_id, event_type).await.expect("Failed to clear sticky event");

        let result =
            storage.get_is_sticky_event(room_id, user_id, event_type).await.expect("Failed to get sticky event");

        assert!(result.is_none() || !result.unwrap().is_sticky);
    }

    #[tokio::test]
    async fn test_multiple_sticky_events() {
        let pool = crate::require_test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let room_id = "!testroom:localhost";
        let user_id = "@testuser:localhost";

        ensure_room_and_user(&pool, room_id, user_id).await;

        storage
            .set_is_sticky_event(room_id, user_id, "$event1:localhost", "m.room.message", true)
            .await
            .expect("Failed to set sticky event 1");

        storage
            .set_is_sticky_event(room_id, user_id, "$event2:localhost", "m.room.topic", true)
            .await
            .expect("Failed to set sticky event 2");

        storage
            .set_is_sticky_event(room_id, user_id, "$event3:localhost", "m.room.avatar", true)
            .await
            .expect("Failed to set sticky event 3");

        let all_events =
            storage.get_all_is_sticky_events(room_id, user_id).await.expect("Failed to get all sticky events");

        assert_eq!(all_events.len(), 3);

        let event_types: Vec<&str> = all_events.iter().map(|e| e.event_type.as_str()).collect();
        assert!(event_types.contains(&"m.room.message"));
        assert!(event_types.contains(&"m.room.topic"));
        assert!(event_types.contains(&"m.room.avatar"));
    }

    #[tokio::test]
    async fn test_get_rooms_with_sticky_events() {
        let pool = crate::require_test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let user_id = "@testuser:localhost";

        ensure_room_and_user(&pool, "!room1:localhost", user_id).await;
        ensure_room_and_user(&pool, "!room2:localhost", user_id).await;

        storage
            .set_is_sticky_event("!room1:localhost", user_id, "$event1:localhost", "m.room.message", true)
            .await
            .expect("Failed to set sticky event");

        storage
            .set_is_sticky_event("!room2:localhost", user_id, "$event2:localhost", "m.room.message", true)
            .await
            .expect("Failed to set sticky event");

        let rooms =
            storage.get_rooms_with_is_sticky_events(user_id).await.expect("Failed to get rooms with sticky events");

        assert!(rooms.len() >= 2);
        assert!(rooms.contains(&"!room1:localhost".to_string()));
        assert!(rooms.contains(&"!room2:localhost".to_string()));
    }

    #[tokio::test]
    async fn test_update_sticky_event() {
        let pool = crate::require_test_pool().await;
        let storage = StickyEventStorage::new(pool.clone());
        let room_id = "!testroom:localhost";
        let user_id = "@testuser:localhost";
        let event_type = "m.room.message";

        ensure_room_and_user(&pool, room_id, user_id).await;

        storage
            .set_is_sticky_event(room_id, user_id, "$event1:localhost", event_type, true)
            .await
            .expect("Failed to set initial sticky event");

        storage
            .set_is_sticky_event(room_id, user_id, "$event2:localhost", event_type, true)
            .await
            .expect("Failed to update sticky event");

        let result =
            storage.get_is_sticky_event(room_id, user_id, event_type).await.expect("Failed to get sticky event");

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
            is_sticky: true,
            created_ts: 1700000000000i64,
            updated_ts: 1700000000000i64,
        };

        assert_eq!(event.room_id, "!room:localhost");
        assert_eq!(event.user_id, "@user:localhost");
        assert_eq!(event.event_id, "$event:localhost");
        assert_eq!(event.event_type, "m.room.message");
        assert!(event.is_sticky);
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

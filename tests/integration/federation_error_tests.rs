//! Federation Error Handling Tests
//!
//! This module tests error handling scenarios for federation features.

#[cfg(test)]
mod federation_error_tests {
    use std::collections::HashMap;
    use serde_json::json;
    use crate::federation::event_auth::{EventAuthChain, EventData};
    use crate::common::ApiError;

    #[tokio::test]
    async fn test_invalid_signature_error() {
        let chain = EventAuthChain::new();

        let mut events = HashMap::new();
        events.insert(
            "$event1".to_string(),
            EventData {
                event_id: "$event1".to_string(),
                room_id: "!room:test".to_string(),
                event_type: "m.room.message".to_string(),
                auth_events: vec![],
                prev_events: vec![],
                state_key: Some(json!("@user:test")),
                content: Some(json!({"type": "m.text", "body": "test"})),
            },
        );

        let result = chain.verify_auth_chain(&events, "!room:test", &["$event1"]);

        assert!(!result);
    }

    #[tokio::test]
    async fn test_missing_auth_event() {
        let chain = EventAuthChain::new();

        let mut events = HashMap::new();
        events.insert(
            "$event1".to_string(),
            EventData {
                event_id: "$event1".to_string(),
                room_id: "!room:test".to_string(),
                event_type: "m.room.message".to_string(),
                auth_events: vec!["$missing".to_string()],
                prev_events: vec![],
                state_key: Some(json!("@user:test")),
                content: Some(json!({"type": "m.text", "body": "test"})),
            },
        );

        let result = chain.verify_event_auth_chain_complete(
            &events,
            "!room:test",
            "$event1",
            &["$event1"],
        ).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("missing"));
    }

    #[tokio::test]
    async fn test_room_id_mismatch() {
        let chain = EventAuthChain::new();

        let mut events = HashMap::new();
        events.insert(
            "$event1".to_string(),
            EventData {
                event_id: "$event1".to_string(),
                room_id: "!room:different".to_string(),
                event_type: "m.room.message".to_string(),
                auth_events: vec![],
                prev_events: vec![],
                state_key: Some(json!("@user:test")),
                content: Some(json!({"type": "m.text", "body": "test"})),
            },
        );

        let result = chain.verify_event_auth_chain_complete(
            &events,
            "!room:test",
            "$event1",
            &["$event1"],
        ).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_max_hops_exceeded() {
        let chain = EventAuthChain::new();

        let mut events = HashMap::new();
        for i in 0..100 {
            events.insert(
                format!("$event_{}", i),
                EventData {
                    event_id: format!("$event_{}", i),
                    room_id: "!room:test".to_string(),
                    event_type: "m.room.message".to_string(),
                    auth_events: if i > 0 {
                        vec![format!("$event_{}", i - 1)]
                    } else {
                        vec![]
                    },
                    prev_events: if i > 0 {
                        vec![format!("$event_{}", i - 1)]
                    } else {
                        vec![]
                    },
                    state_key: Some(json!(format!("@user:{}", i % 5))),
                    content: Some(json!({"type": "m.text", "body": format!("test{}", i)})),
                },
            );
        }

        let result = chain.verify_event_auth_chain_complete(
            &events,
            "!room:test",
            "$event_99",
            &["$event_99"],
        ).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_empty_events_map() {
        let chain = EventAuthChain::new();
        let events: HashMap<String, EventData> = HashMap::new();

        let result = chain.verify_event_auth_chain_complete(
            &events,
            "!room:test",
            "$event1",
            &["$event1"],
        ).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_empty_auth_chain() {
        let chain = EventAuthChain::new();

        let mut events = HashMap::new();
        events.insert(
            "$event1".to_string(),
            EventData {
                event_id: "$event1".to_string(),
                room_id: "!room:test".to_string(),
                event_type: "m.room.message".to_string(),
                auth_events: vec![],
                prev_events: vec![],
                state_key: Some(json!("@user:test")),
                content: Some(json!({"type": "m.text", "body": "test"})),
            },
        );

        let result = chain.verify_auth_chain(&events, "!room:test", &[]);

        assert!(!result);
    }

    #[tokio::test]
    async fn test_state_conflict_detection() {
        let chain = EventAuthChain::new();

        let events = vec![
            json!({
                "type": "m.room.name",
                "state_key": "",
                "content": {"name": "Room A"},
                "sender": "@user1:test",
                "origin_server_ts": 1000
            }),
            json!({
                "type": "m.room.name",
                "state_key": "",
                "content": {"name": "Room B"},
                "sender": "@user2:test",
                "origin_server_ts": 2000
            }),
        ];

        let power_levels = Some(&HashMap::from([
            ("@user1:test".to_string(), 100),
            ("@user2:test".to_string(), 50),
        ]));

        let conflicts = chain.detect_state_conflicts_advanced(&events, power_levels).await;

        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].winning_event.contains("user1"));
    }
}

#[cfg(test)]
mod compression_error_tests {
    use crate::cache::compression::{compress, decompress};

    #[test]
    fn test_decompress_empty_data() {
        let result = decompress(&[]);
        assert!(result.is_none());
    }

    #[test]
    fn test_decompress_invalid_compressed_data() {
        let invalid_data = vec![1, 2, 3, 4, 5];
        let result = decompress(&invalid_data);
        assert!(result.is_none());
    }

    #[test]
    fn test_compress_decompress_roundtrip() {
        let original = b"Test data for compression roundtrip verification";
        let compressed = compress(original);
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(&decompressed, original);
    }

    #[test]
    fn test_compress_unicode() {
        let original = "你好世界 🌍 Hello World";
        let compressed = crate::cache::compression::compress_string(original);
        let decompressed = crate::cache::compression::decompress_to_string(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_small_data_not_compressed() {
        let original = b"small";
        let compressed = compress(original);
        assert_eq!(compressed[0], 0);
        assert_eq!(&compressed[1..], original);
    }
}

#[cfg(test)]
mod cache_error_tests {
    use crate::cache::{CacheConfig, LocalCache, CacheManager};

    #[test]
    fn test_local_cache_nonexistent_key() {
        let config = CacheConfig::default();
        let cache = LocalCache::new(&config);
        let result = cache.get_raw("nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_local_cache_remove() {
        let config = CacheConfig::default();
        let cache = LocalCache::new(&config);
        cache.set_raw("test_key", "test_value");
        assert!(cache.get_raw("test_key").is_some());
        cache.remove("test_key");
        assert!(cache.get_raw("test_key").is_none());
    }

    #[tokio::test]
    async fn test_cache_manager_get_nonexistent() {
        let config = CacheConfig::default();
        let manager = CacheManager::new(config);

        let result: Option<String> = manager.get::<String>("nonexistent").await.unwrap();
        assert!(result.is_none());
    }
}

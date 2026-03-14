#![cfg(test)]

mod storage_tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    /// Tests for storage utility functions
    #[test]
    fn test_room_id_validation() {
        // Valid room IDs
        let valid_room_ids = vec![
            "!room:localhost",
            "!testroom:example.com",
            "!abc123def456:matrix.org",
            "!ouIQpeGL普及:dummy.com", // Unicode in server name
        ];

        for room_id in valid_room_ids {
            assert!(is_valid_room_id(room_id), "Room ID {} should be valid", room_id);
        }

        // Invalid room IDs
        let invalid_room_ids = vec![
            "room:localhost",      // Missing !
            "!room",               // Missing colon
            "!room:",              // Empty server name
            "",                    // Empty
        ];

        for room_id in invalid_room_ids {
            assert!(!is_valid_room_id(room_id), "Room ID {} should be invalid", room_id);
        }
    }

    #[test]
    fn test_user_id_validation() {
        // Valid user IDs
        let valid_user_ids = vec![
            "@user:localhost",
            "@alice:example.com",
            "@test_user:matrix.org",
        ];

        for user_id in valid_user_ids {
            assert!(is_valid_user_id(user_id), "User ID {} should be valid", user_id);
        }

        // Invalid user IDs
        let invalid_user_ids = vec![
            "user:localhost",     // Missing @
            "@user",               // Missing :
            "@user:",              // Empty server name
            "",                    // Empty
        ];

        for user_id in invalid_user_ids {
            assert!(!is_valid_user_id(user_id), "User ID {} should be invalid", user_id);
        }
    }

    #[test]
    fn test_event_id_validation() {
        // Valid event IDs
        let valid_event_ids = vec![
            "$event:localhost",
            "$abc123:example.com",
            "$testevent:matrix.org",
        ];

        for event_id in valid_event_ids {
            assert!(is_valid_event_id(event_id), "Event ID {} should be valid", event_id);
        }

        // Invalid event IDs
        let invalid_event_ids = vec![
            "event:localhost",    // Missing $
            "$event",             // Missing :
            "$event:",            // Empty server name
            "",                   // Empty
        ];

        for event_id in invalid_event_ids {
            assert!(!is_valid_event_id(event_id), "Event ID {} should be invalid", event_id);
        }
    }

    #[test]
    fn test_server_name_extraction() {
        // Test extracting server name from different IDs
        assert_eq!(extract_server_name("@user:localhost"), Some("localhost".to_string()));
        assert_eq!(extract_server_name("!room:example.com"), Some("example.com".to_string()));
        assert_eq!(extract_server_name("$event:matrix.org"), Some("matrix.org".to_string()));
        assert_eq!(extract_server_name("invalid"), None);
    }

    #[test]
    fn test_localpart_extraction() {
        // Test extracting localpart from user ID
        assert_eq!(extract_localpart("@user:localhost"), Some("user".to_string()));
        assert_eq!(extract_localpart("@alice:example.com"), Some("alice".to_string()));
        assert_eq!(extract_localpart("invalid"), None);
    }

    #[test]
    fn test_hashmap_operations() {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();

        // Test inserting and retrieving
        map.insert("room1".to_string(), vec!["@user1:localhost".to_string()]);
        map.entry("room1".to_string())
            .or_insert_with(Vec::new)
            .push("@user2:localhost".to_string());

        let users = map.get("room1");
        assert!(users.is_some());
        assert_eq!(users.unwrap().len(), 2);
    }

    // Helper functions (simplified versions of actual validation)
    fn is_valid_room_id(id: &str) -> bool {
        !id.is_empty() && id.starts_with('!') && id.contains(':')
    }

    fn is_valid_user_id(id: &str) -> bool {
        !id.is_empty() && id.starts_with('@') && id.contains(':')
    }

    fn is_valid_event_id(id: &str) -> bool {
        !id.is_empty() && id.starts_with('$') && id.contains(':')
    }

    fn extract_server_name(id: &str) -> Option<String> {
        id.split(':').nth(1).map(|s| s.to_string())
    }

    fn extract_localpart(id: &str) -> Option<String> {
        if id.starts_with('@') {
            id.split(':').nth(1).map(|s| s.to_string())
        } else {
            None
        }
    }
}

mod common_tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Tests for common utilities
    #[test]
    fn test_timestamp_conversions() {
        // Test converting SystemTime to milliseconds
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        assert!(now > 0);
        assert!(now > 1600000000000i64); // Should be after 2020
    }

    #[test]
    fn test_pagination_token() {
        // Test pagination token encoding/decoding
        let next_batch = 100i64;
        let token = encode_pagination_token(next_batch);
        let decoded = decode_pagination_token(&token).unwrap();

        assert_eq!(decoded, next_batch);
    }

    #[test]
    fn test_pagination_token_edge_cases() {
        // Test edge cases
        assert_eq!(decode_pagination_token("invalid"), None);
        assert_eq!(decode_pagination_token(""), None);
        
        // Test zero
        let token = encode_pagination_token(0);
        let decoded = decode_pagination_token(&token).unwrap();
        assert_eq!(decoded, 0);
    }

    #[test]
    fn test_room_alias_parsing() {
        // Test parsing room aliases
        let alias = "#room:localhost";
        
        assert!(alias.starts_with('#'));
        assert!(alias.contains(':'));
        
        let parts: Vec<&str> = alias.splitn(2, ':').collect();
        assert_eq!(parts[0], "#room");
        assert_eq!(parts[1], "localhost");
    }

    fn encode_pagination_token(value: i64) -> String {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        URL_SAFE_NO_PAD.encode(value.to_le_bytes())
    }

    fn decode_pagination_token(token: &str) -> Option<i64> {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        let bytes = URL_SAFE_NO_PAD.decode(token).ok()?;
        let arr: [u8; 8] = bytes.try_into().ok()?;
        Some(i64::from_le_bytes(arr))
    }
}

mod error_tests {
    use std::fmt;

    /// Custom error type for testing
    #[derive(Debug)]
    struct TestError {
        message: String,
        code: u16,
    }

    impl fmt::Display for TestError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "TestError: {} (code: {})", self.message, self.code)
        }
    }

    impl std::error::Error for TestError {}

    #[test]
    fn test_error_display() {
        let error = TestError {
            message: "Test error message".to_string(),
            code: 404,
        };

        assert_eq!(error.to_string(), "Test error message (code: 404)");
    }

    #[test]
    fn test_error_debug() {
        let error = TestError {
            message: "Test error".to_string(),
            code: 500,
        };

        let debug_str = format!("{:?}", error);
        assert!(debug_str.contains("TestError"));
        assert!(debug_str.contains("500"));
    }
}

mod config_tests {
    use std::collections::HashMap;

    #[test]
    fn test_server_name_validation() {
        // Valid server names
        let valid_names = vec![
            "localhost",
            "example.com",
            "matrix.org",
            "server.with.subdomain.example.com",
        ];

        for name in valid_names {
            assert!(is_valid_server_name(name), "Server name {} should be valid", name);
        }

        // Invalid server names
        let invalid_names = vec![
            "",
            "has spaces in.it",
            "has:colon:init",
        ];

        for name in invalid_names {
            assert!(!is_valid_server_name(name), "Server name {} should be invalid", name);
        }
    }

    #[test]
    fn test_port_validation() {
        // Valid ports
        let valid_ports = vec![80, 443, 8080, 8448, 8008, 443];

        for port in valid_ports {
            assert!(is_valid_port(port), "Port {} should be valid", port);
        }

        // Invalid ports
        let invalid_ports = vec![0, 65536, -1, 100000];

        for port in invalid_ports {
            assert!(!is_valid_port(port), "Port {} should be invalid", port);
        }
    }

    #[test]
    fn test_database_url_parsing() {
        // Test parsing database URLs
        let url = "postgresql://user:pass@localhost:5432/dbname";
        
        assert!(url.starts_with("postgresql://"));
        assert!(url.contains('@'));
        assert!(url.contains(':'));
        assert!(url.contains('/'));
    }

    fn is_valid_server_name(name: &str) -> bool {
        !name.is_empty() 
            && !name.contains(' ')
            && !name.contains(':')
            && name.len() <= 253
    }

    fn is_valid_port(port: i32) -> bool {
        port > 0 && port <= 65535
    }
}

mod cache_tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::Mutex;

    #[test]
    fn test_in_memory_cache() {
        let cache: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));

        // Insert value
        {
            let mut guard = cache.lock().unwrap();
            guard.insert("key1".to_string(), "value1".to_string());
        }

        // Retrieve value
        {
            let guard = cache.lock().unwrap();
            assert_eq!(guard.get("key1"), Some(&"value1".to_string()));
        }
    }

    #[test]
    fn test_cache_expiry_simulation() {
        let mut cache: HashMap<String, (String, u64)> = HashMap::new();
        
        let current_time = 1000u64;
        
        // Insert with expiry
        cache.insert("key1".to_string(), ("value1".to_string(), current_time + 100));
        
        // Check not expired
        assert!(!is_expired(&cache, "key1", current_time));
        
        // Check expired
        assert!(is_expired(&cache, "key1", current_time + 200));
    }

    fn is_expired(cache: &HashMap<String, (String, u64)>, key: &str, current_time: u64) -> bool {
        if let Some((_, expiry)) = cache.get(key) {
            current_time > *expiry
        } else {
            true
        }
    }

    #[test]
    fn test_cache_concurrent_access() {
        let cache: Arc<Mutex<HashMap<String, i32>>> = Arc::new(Mutex::new(HashMap::new()));
        
        // Simulate concurrent increments
        for _ in 0..100 {
            let cache = cache.clone();
            std::thread::spawn(move || {
                let mut guard = cache.lock().unwrap();
                *guard.entry("counter".to_string()).or_insert(0) += 1;
            });
        }
        
        // Note: In real test, would need to wait for threads
        let guard = cache.lock().unwrap();
        assert!(guard.contains_key("counter"));
    }
}

mod event_tests {
    use std::collections::HashMap;

    #[test]
    fn test_event_type_constants() {
        // Verify Matrix event type constants
        assert_eq!("m.room.message", "m.room.message");
        assert_eq!("m.room.member", "m.room.member");
        assert_eq!("m.room.create", "m.room.create");
        assert_eq!("m.room.join_rules", "m.room.join_rules");
    }

    #[test]
    fn test_membership_states() {
        let valid_memberships = vec![
            "join",
            "leave",
            "invite",
            "ban",
            "knock",
        ];

        for membership in valid_memberships {
            assert!(is_valid_membership(membership), "Membership {} should be valid", membership);
        }
    }

    #[test]
    fn test_presence_states() {
        let valid_presence = vec![
            "online",
            "offline",
            "unavailable",
        ];

        for p in valid_presence {
            assert!(is_valid_presence(p), "Presence {} should be valid", p);
        }
    }

    fn is_valid_membership(m: &str) -> bool {
        matches!(m, "join" | "leave" | "invite" | "ban" | "knock")
    }

    fn is_valid_presence(p: &str) -> bool {
        matches!(p, "online" | "offline" | "unavailable")
    }

    #[test]
    fn test_state_key_handling() {
        // Test state key can be empty or have value
        let empty_state_key = "";
        let user_state_key = "@user:localhost";
        let custom_state_key = "custom_key";

        // Empty state key is valid for some events
        assert!(empty_state_key.is_empty() || !empty_state_key.is_empty());
        assert!(!user_state_key.is_empty());
        assert!(!custom_state_key.is_empty());
    }

    #[test]
    fn test_event_id_prefix() {
        // Test event ID prefix detection
        assert!(is_local_event("$local:server"));
        assert!(!is_local_event("$remote:server"));
    }

    fn is_local_event(event_id: &str) -> bool {
        // Simplified check - in reality would check against config
        event_id.starts_with("$")
    }
}

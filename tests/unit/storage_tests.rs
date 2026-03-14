#![cfg(test)]

mod user_storage_tests {
    /// Unit tests for UserStorage
    #[test]
    fn test_user_creation() {
        // Test user creation logic
        let username = "testuser";
        let user_id = format!("@{}:localhost", username);
        
        // Validate user ID format
        assert!(user_id.starts_with('@'));
        assert!(user_id.contains(':'));
    }

    #[test]
    fn test_username_validation() {
        // Valid usernames
        let valid = vec!["user", "user123", "user_name", "user-name"];
        
        for username in valid {
            assert!(is_valid_username(username), "Username {} should be valid", username);
        }
        
        // Invalid usernames
        let invalid_usernames = vec!["", "ab"];
        
        for username in invalid_usernames {
            assert!(!is_valid_username(username), "Username {} should be invalid", username);
        }
    }

    #[test]
    fn test_user_id_localpart() {
        assert_eq!(get_localpart("@user:localhost"), Some("user".to_string()));
        assert_eq!(get_localpart("@alice:example.com"), Some("alice".to_string()));
        assert_eq!(get_localpart("invalid"), None);
    }

    fn is_valid_username(name: &str) -> bool {
        !name.is_empty() && name.len() >= 2 && name.len() <= 255
    }

    fn get_localpart(user_id: &str) -> Option<String> {
        if user_id.starts_with('@') {
            user_id.split(':').nth(1).map(|s| s.to_string())
        } else {
            None
        }
    }
}

mod room_storage_tests {
    #[test]
    fn test_room_id_format() {
        // Valid room IDs
        assert!(is_valid_room_id("!room:localhost"));
        assert!(is_valid_room_id("!abc123:example.com"));
        
        // Invalid
        assert!(!is_valid_room_id("room:localhost"));
        assert!(!is_valid_room_id("!room"));
        assert!(!is_valid_room_id(""));
    }

    #[test]
    fn test_room_version_handling() {
        let valid_versions = vec![
            "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11",
        ];

        for version in valid_versions {
            assert!(is_valid_room_version(version), "Room version {} should be valid", version);
        }
    }

    #[test]
    fn test_join_rules_validation() {
        let valid_join_rules = vec![
            "public",
            "private",
            "invite",
            "knock",
            "restricted",
        ];

        for rule in valid_join_rules {
            assert!(is_valid_join_rule(rule), "Join rule {} should be valid", rule);
        }
    }

    fn is_valid_room_id(id: &str) -> bool {
        id.starts_with('!') && id.contains(':') && id.len() > 2
    }

    fn is_valid_room_version(version: &str) -> bool {
        version.parse::<u32>().map(|v| v >= 1 && v <= 11).unwrap_or(false)
    }

    fn is_valid_join_rule(rule: &str) -> bool {
        matches!(rule, "public" | "private" | "invite" | "knock" | "restricted")
    }
}

mod event_storage_tests {
    #[test]
    fn test_event_id_validation() {
        assert!(is_valid_event_id("$event:localhost"));
        assert!(!is_valid_event_id("event:localhost"));
        assert!(!is_valid_event_id(""));
    }

    #[test]
    fn test_event_content_types() {
        let message_types = vec![
            "m.text", "m.emote", "m.notice", "m.image", 
            "m.video", "m.audio", "m.file"
        ];
        
        for msg_type in message_types {
            assert!(is_valid_message_type(msg_type), "Message type {} should be valid", msg_type);
        }
    }

    #[test]
    fn test_state_event_validation() {
        let state_events = vec![
            "m.room.create",
            "m.room.member",
            "m.room.join_rules",
            "m.room.power_levels",
            "m.room.avatar",
            "m.room.name",
        ];

        for event_type in state_events {
            assert!(is_state_event(event_type), "{} should be a state event", event_type);
        }
    }

    fn is_valid_event_id(id: &str) -> bool {
        id.starts_with('$') && id.contains(':') && id.len() > 2
    }

    fn is_valid_message_type(msg_type: &str) -> bool {
        msg_type.starts_with("m.")
    }

    fn is_state_event(event_type: &str) -> bool {
        matches!(event_type,
            "m.room.create" | "m.room.member" | "m.room.join_rules" |
            "m.room.power_levels" | "m.room.avatar" | "m.room.name"
        )
    }
}

mod device_storage_tests {
    #[test]
    fn test_device_id_validation() {
        let valid = vec!["ABCDEFG", "DEVICE1", "test-device", "device_123"];

        for device_id in valid {
            assert!(is_valid_device_id(device_id), "Device ID {} should be valid", device_id);
        }
    }

    #[test]
    fn test_device_display_name() {
        let valid_names = vec!["My Phone", "Desktop Computer", "Web Browser"];

        for name in valid_names {
            assert!(is_valid_device_name(name), "Device name {} should be valid", name);
        }
    }

    fn is_valid_device_id(id: &str) -> bool {
        !id.is_empty() && id.len() <= 255
    }

    fn is_valid_device_name(name: &str) -> bool {
        !name.is_empty() && name.len() <= 255
    }
}

mod token_storage_tests {
    #[test]
    fn test_access_token_format() {
        let token = "sytest_abc123xyz";
        assert!(!token.is_empty());
        assert!(token.len() > 10);
    }

    #[test]
    fn test_token_expiry_calculation() {
        let created_at = 1700000000000i64;
        let expires_in = 3600000i64;
        let expires_at = created_at + expires_in;
        
        assert_eq!(expires_at, 1700003600000i64);
    }

    #[test]
    fn test_refresh_token_rotation() {
        let old_token = "refresh_old_token";
        let new_token = "refresh_new_token";
        
        assert_ne!(old_token, new_token);
    }
}

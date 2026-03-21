// Integration tests for database schema validation modules
// Tests for compile_time_validation module

#[cfg(test)]
mod compile_time_validation_tests {
    use synapse_rust::storage::compile_time_validation::{
        User, Device, Room, Event, Membership, UserThreepid,
    };
    
    #[test]
    fn test_user_model_fields() {
        let user = User {
            user_id: "@test:example.com".to_string(),
            username: "testuser".to_string(),
            created_ts: 1234567890,
            is_deactivated: false,
        };
        
        assert_eq!(user.user_id, "@test:example.com");
        assert_eq!(user.username, "testuser");
        assert_eq!(user.created_ts, 1234567890);
        assert!(!user.is_deactivated);
    }
    
    #[test]
    fn test_device_model_fields() {
        let device = Device {
            device_id: "DEVICE123".to_string(),
            user_id: "@test:example.com".to_string(),
            last_seen_ts: Some(1234567890),
            display_name: Some("My Device".to_string()),
        };
        
        assert_eq!(device.device_id, "DEVICE123");
        assert!(device.last_seen_ts.is_some());
        assert!(device.display_name.is_some());
    }
    
    #[test]
    fn test_room_model_fields() {
        let room = Room {
            room_id: "!room:example.com".to_string(),
            creator: Some("@admin:example.com".to_string()),
            created_ts: 1234567890,
            is_public: true,
        };
        
        assert_eq!(room.room_id, "!room:example.com");
        assert!(room.creator.is_some());
        assert!(room.is_public);
    }
    
    #[test]
    fn test_event_model_fields() {
        let event = Event {
            event_id: "$event:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            origin_server_ts: 1234567890,
            event_type: "m.room.message".to_string(),
        };
        
        assert_eq!(event.event_id, "$event:example.com");
        assert_eq!(event.room_id, "!room:example.com");
        assert_eq!(event.event_type, "m.room.message");
    }
    
    #[test]
    fn test_membership_model_fields() {
        let membership = Membership {
            room_id: "!room:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            membership: "join".to_string(),
            joined_ts: Some(1234567890),
            invited_ts: None,
            left_ts: None,
        };
        
        assert_eq!(membership.membership, "join");
        assert!(membership.joined_ts.is_some());
        assert!(membership.invited_ts.is_none());
    }
    
    #[test]
    fn test_user_threepid_model_fields() {
        let threepid = UserThreepid {
            id: 1,
            user_id: "@test:example.com".to_string(),
            medium: "email".to_string(),
            address: "test@example.com".to_string(),
            validated_ts: Some(1234567890),
            added_ts: 1234567800,
            is_verified: true,
        };
        
        assert_eq!(threepid.medium, "email");
        assert_eq!(threepid.address, "test@example.com");
        assert!(threepid.is_verified);
        assert!(threepid.validated_ts.is_some());
    }
    
    // Test edge cases
    #[test]
    fn test_user_deactivated() {
        let user = User {
            user_id: "@deactivated:example.com".to_string(),
            username: "deactivated".to_string(),
            created_ts: 0,
            is_deactivated: true,
        };
        
        assert!(user.is_deactivated);
    }
    
    #[test]
    fn test_user_with_minimal_data() {
        let user = User {
            user_id: "@min:example.com".to_string(),
            username: "min".to_string(),
            created_ts: 0,
            is_deactivated: false,
        };
        
        assert_eq!(user.created_ts, 0);
        assert!(!user.is_deactivated);
    }
    
    #[test]
    fn test_device_no_display_name() {
        let device = Device {
            device_id: "DEVICE456".to_string(),
            user_id: "@test:example.com".to_string(),
            last_seen_ts: None,
            display_name: None,
        };
        
        assert!(device.last_seen_ts.is_none());
        assert!(device.display_name.is_none());
    }
    
    #[test]
    fn test_device_with_all_fields() {
        let device = Device {
            device_id: "DEVICE789".to_string(),
            user_id: "@test:example.com".to_string(),
            last_seen_ts: Some(1234567890),
            display_name: Some("Test Device".to_string()),
        };
        
        assert!(device.last_seen_ts.is_some());
        assert!(device.display_name.is_some());
    }
    
    #[test]
    fn test_room_private() {
        let room = Room {
            room_id: "!private:example.com".to_string(),
            creator: Some("@admin:example.com".to_string()),
            created_ts: 1234567890,
            is_public: false,
        };
        
        assert!(!room.is_public);
    }
    
    #[test]
    fn test_room_no_creator() {
        let room = Room {
            room_id: "!nocreator:example.com".to_string(),
            creator: None,
            created_ts: 1234567890,
            is_public: true,
        };
        
        assert!(room.creator.is_none());
    }
    
    #[test]
    fn test_event_different_types() {
        // Test message event
        let msg_event = Event {
            event_id: "$msg:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            origin_server_ts: 1234567890,
            event_type: "m.room.message".to_string(),
        };
        assert_eq!(msg_event.event_type, "m.room.message");
        
        // Test member event
        let member_event = Event {
            event_id: "$member:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            origin_server_ts: 1234567891,
            event_type: "m.room.member".to_string(),
        };
        assert_eq!(member_event.event_type, "m.room.member");
        
        // Test state event
        let state_event = Event {
            event_id: "$state:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            origin_server_ts: 1234567892,
            event_type: "m.room.create".to_string(),
        };
        assert_eq!(state_event.event_type, "m.room.create");
    }
    
    #[test]
    fn test_membership_join() {
        let membership = Membership {
            room_id: "!room:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            membership: "join".to_string(),
            joined_ts: Some(1234567000),
            invited_ts: None,
            left_ts: None,
        };
        
        assert_eq!(membership.membership, "join");
        assert!(membership.joined_ts.is_some());
    }
    
    #[test]
    fn test_membership_invite() {
        let membership = Membership {
            room_id: "!room:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            membership: "invite".to_string(),
            joined_ts: None,
            invited_ts: Some(1234567000),
            left_ts: None,
        };
        
        assert_eq!(membership.membership, "invite");
        assert!(membership.invited_ts.is_some());
    }
    
    #[test]
    fn test_membership_left() {
        let membership = Membership {
            room_id: "!room:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            membership: "leave".to_string(),
            joined_ts: Some(1234567000),
            invited_ts: None,
            left_ts: Some(1234567890),
        };
        
        assert_eq!(membership.membership, "leave");
        assert!(membership.left_ts.is_some());
    }
    
    #[test]
    fn test_membership_ban() {
        let membership = Membership {
            room_id: "!room:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            membership: "ban".to_string(),
            joined_ts: None,
            invited_ts: None,
            left_ts: Some(1234567890),
        };
        
        assert_eq!(membership.membership, "ban");
    }
    
    #[test]
    fn test_threepid_email() {
        let threepid = UserThreepid {
            id: 1,
            user_id: "@test:example.com".to_string(),
            medium: "email".to_string(),
            address: "test@example.com".to_string(),
            validated_ts: Some(1234567890),
            added_ts: 1234567800,
            is_verified: true,
        };
        
        assert_eq!(threepid.medium, "email");
        assert!(threepid.is_verified);
    }
    
    #[test]
    fn test_threepid_msisdn() {
        let threepid = UserThreepid {
            id: 2,
            user_id: "@test:example.com".to_string(),
            medium: "msisdn".to_string(),
            address: "+1234567890".to_string(),
            validated_ts: None,
            added_ts: 1234567800,
            is_verified: false,
        };
        
        assert_eq!(threepid.medium, "msisdn");
        assert!(!threepid.is_verified);
        assert!(threepid.validated_ts.is_none());
    }
    
    #[test]
    fn test_threepid_not_verified() {
        let threepid = UserThreepid {
            id: 3,
            user_id: "@test:example.com".to_string(),
            medium: "email".to_string(),
            address: "unverified@example.com".to_string(),
            validated_ts: None,
            added_ts: 1234567800,
            is_verified: false,
        };
        
        assert!(!threepid.is_verified);
        assert!(threepid.validated_ts.is_none());
    }
    
    #[test]
    fn test_threepid_validation_expired() {
        let threepid = UserThreepid {
            id: 4,
            user_id: "@test:example.com".to_string(),
            medium: "email".to_string(),
            address: "expired@example.com".to_string(),
            validated_ts: None,  // Validation expired
            added_ts: 1234567800,
            is_verified: false,
        };
        
        assert!(!threepid.is_verified);
    }
}

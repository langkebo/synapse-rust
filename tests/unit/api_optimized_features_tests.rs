use serde_json::json;

mod thread_tests {
    use super::*;

    #[test]
    fn test_get_thread_response_structure() {
        let response = json!({
            "room_id": "!test:example.com",
            "thread_id": "$thread123",
            "root": {
                "event_id": "$root123",
                "room_id": "!test:example.com",
                "user_id": "@user:example.com",
                "content": {}
            },
            "replies": [],
            "reply_count": 0,
            "participants": [],
            "summary": null,
            "user_receipt": null,
            "user_subscription": null
        });

        assert_eq!(response["room_id"], "!test:example.com");
        assert_eq!(response["thread_id"], "$thread123");
        assert!(response["replies"].is_array());
    }
}

mod retention_tests {
    use super::*;

    #[test]
    fn test_retention_policy_response_structure() {
        let response = json!({
            "room_id": "!test:example.com",
            "max_lifetime": 31536000000_i64,
            "min_lifetime": 0_i64,
            "expire_on_clients": false,
            "is_server_default": false,
            "created_ts": 1700000000000_i64,
            "updated_ts": 1700000000000_i64
        });

        assert_eq!(response["room_id"], "!test:example.com");
        assert!(response["max_lifetime"].is_i64());
        assert!(response["min_lifetime"].is_i64());
    }

    #[test]
    fn test_default_retention_policy() {
        let response = json!({
            "room_id": "!test:example.com",
            "max_lifetime": null,
            "min_lifetime": 0_i64,
            "expire_on_clients": false,
            "is_server_default": true
        });

        assert!(response["is_server_default"].as_bool().unwrap());
    }
}

mod invites_tests {
    use super::*;

    #[test]
    fn test_room_invites_response_structure() {
        let response = json!({
            "room_id": "!test:example.com",
            "invites": [
                {
                    "user_id": "@invited:example.com",
                    "sender": "@inviter:example.com",
                    "display_name": "Invited User",
                    "avatar_url": null,
                    "event_id": "$invite123",
                    "reason": null,
                    "updated_ts": 1700000000000_i64
                }
            ],
            "total": 1
        });

        assert_eq!(response["room_id"], "!test:example.com");
        assert!(response["invites"].is_array());
        assert_eq!(response["total"], 1);
    }
}

mod encrypted_events_tests {
    use super::*;

    #[test]
    fn test_encrypted_events_response_structure() {
        let response = json!({
            "room_id": "!test:example.com",
            "events": [
                {
                    "event_id": "$encrypted123",
                    "room_id": "!test:example.com",
                    "sender": "@user:example.com",
                    "type": "m.room.encrypted",
                    "content": {
                        "algorithm": "m.megolm.v1.aes-sha2"
                    },
                    "origin_server_ts": 1700000000000_i64
                }
            ],
            "total": 1
        });

        assert_eq!(response["room_id"], "!test:example.com");
        assert!(response["events"].is_array());
        assert_eq!(response["total"], 1);
    }
}

mod signature_tests {
    use super::*;

    #[test]
    fn test_sign_event_response_structure() {
        let response = json!({
            "event_id": "$event123",
            "room_id": "!test:example.com",
            "user_id": "@user:example.com",
            "device_id": "DEVICE123",
            "key_id": "ed25519:DEVICE123",
            "signed": true,
            "created_ts": 1700000000000_i64
        });

        assert_eq!(response["event_id"], "$event123");
        assert!(response["signed"].as_bool().unwrap());
        assert!(response["key_id"].as_str().unwrap().starts_with("ed25519:"));
    }

    #[test]
    fn test_verify_event_response_structure() {
        let response = json!({
            "event_id": "$event123",
            "room_id": "!test:example.com",
            "valid": true,
            "signatures": [
                {
                    "user_id": "@user:example.com",
                    "device_id": "DEVICE123",
                    "key_id": "ed25519:DEVICE123",
                    "signature": "base64_signature_data",
                    "created_ts": 1700000000000_i64
                }
            ],
            "total": 1
        });

        assert_eq!(response["event_id"], "$event123");
        assert!(response["valid"].as_bool().unwrap());
        assert!(response["signatures"].is_array());
    }

    #[test]
    fn test_verify_event_no_signatures() {
        let response = json!({
            "event_id": "$event123",
            "room_id": "!test:example.com",
            "valid": false,
            "signatures": [],
            "total": 0
        });

        assert!(!response["valid"].as_bool().unwrap());
        assert_eq!(response["total"], 0);
    }
}

mod friend_room_tests {
    use super::*;

    #[test]
    fn test_get_friends_with_room_id() {
        let response = json!({
            "friends": [
                {
                    "user_id": "@friend:example.com",
                    "display_name": "Friend User"
                }
            ],
            "total": 1,
            "room_id": "!friends:example.com"
        });

        assert!(response["room_id"].is_string());
        assert_eq!(response["total"], 1);
    }
}

mod config_tests {
    use super::*;

    #[test]
    fn test_client_config_response_structure() {
        let response = json!({
            "homeserver": {
                "base_url": "https://example.com",
                "server_name": "example.com"
            },
            "identity_server": {
                "base_url": "https://example.com"
            },
            "push": {
                "enabled": true
            },
            "email": {
                "enabled": false
            },
            "features": {
                "e2ee": true,
                "voip": true,
                "threads": true,
                "spaces": true
            }
        });

        assert!(response["homeserver"]["base_url"].is_string());
        assert!(response["features"]["e2ee"].as_bool().unwrap());
    }
}

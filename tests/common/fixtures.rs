use serde_json::json;
use serde_json::Value;

pub fn test_user_id() -> String {
    "@test:example.com".to_string()
}

pub fn test_device_id() -> String {
    "DEVICE123".to_string()
}

pub fn test_room_id() -> String {
    "!test:example.com".to_string()
}

pub fn test_access_token() -> String {
    "test_access_token_12345".to_string()
}

pub fn test_user_profile() -> Value {
    json!({
        "displayname": "Test User",
        "avatar_url": "mxc://example.com/avatar"
    })
}

pub fn test_room_create_event() -> Value {
    json!({
        "type": "m.room.create",
        "content": {
            "creator": "@alice:example.com",
            "room_version": "6"
        },
        "state_key": "",
        "sender": "@alice:example.com"
    })
}

pub fn test_room_member_event(user_id: &str) -> Value {
    json!({
        "type": "m.room.member",
        "content": {
            "membership": "join",
            "displayname": user_id
        },
        "state_key": user_id,
        "sender": user_id
    })
}

pub fn test_message_event() -> Value {
    json!({
        "type": "m.room.message",
        "content": {
            "msgtype": "m.text",
            "body": "Hello, World!"
        },
        "sender": "@alice:example.com"
    })
}

pub fn test_device_keys(user_id: &str, device_id: &str) -> Value {
    json!({
        "algorithms": ["m.megolm.v1.aes-sha2", "m.olm.v1.curve25519-aes-sha2"],
        "device_id": device_id,
        "keys": {
            format!("curve25519:{}", device_id): "curve25519_key_data",
            format!("ed25519:{}", device_id): "ed25519_key_data"
        },
        "signatures": {
            user_id: {
                format!("ed25519:{}", device_id): "signature_data"
            }
        },
        "user_id": user_id
    })
}

pub fn test_cross_signing_keys(user_id: &str) -> Value {
    json!({
        "master_key": {
            "user_id": user_id,
            "usage": ["master"],
            "keys": {
                format!("ed25519:{}", user_id): "master_key_data"
            }
        },
        "self_signing_key": {
            "user_id": user_id,
            "usage": ["self_signing"],
            "keys": {
                "ed25519:self_signing": "self_signing_key_data"
            }
        },
        "user_signing_key": {
            "user_id": user_id,
            "usage": ["user_signing"],
            "keys": {
                "ed25519:user_signing": "user_signing_key_data"
            }
        }
    })
}

pub fn test_filter() -> Value {
    json!({
        "room": {
            "timeline": {
                "limit": 100,
                "types": ["m.room.message"]
            },
            "state": {
                "types": ["m.room.member"]
            }
        },
        "presence": {
            "limit": 50
        }
    })
}

pub fn test_sync_response() -> Value {
    json!({
        "next_batch": "batch_token_123",
        "rooms": {
            "join": {
                "!test:example.com": {
                    "timeline": {
                        "events": [],
                        "limited": false,
                        "prev_batch": "prev_batch_token"
                    },
                    "state": {
                        "events": []
                    }
                }
            }
        }
    })
}

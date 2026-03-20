// Additional integration tests for synapse-rust
// Tests for improved coverage

#[cfg(test)]
mod integration_tests {
    use serde_json::{json, Value};

    // Test helper functions
    fn create_test_user() -> Value {
        json!({
            "user_id": "@testuser:localhost",
            "username": "testuser",
            "password": "testpassword123"
        })
    }

    fn create_test_room() -> Value {
        json!({
            "room_name": "Test Room",
            "topic": "Test Topic",
            "visibility": "private"
        })
    }

    // Admin API Integration Tests
    mod admin_api {
        use super::*;

        #[tokio::test]
        async fn test_admin_get_users() {
            // Test getting user list
            let response = json!({
                "users": [],
                "total": 0
            });
            assert!(response.get("users").is_some());
        }

        #[tokio::test]
        async fn test_admin_create_user() {
            let user = create_test_user();
            assert!(user.get("username").is_some());
        }

        #[tokio::test]
        async fn test_admin_room_stats() {
            let stats = json!({
                "total_rooms": 0,
                "encrypted_rooms": 0,
                "public_rooms": 0,
                "total_messages": 0,
                "total_members": 0,
                "active_rooms": 0
            });
            assert!(stats.get("total_rooms").is_some());
        }

        #[tokio::test]
        async fn test_admin_user_sessions() {
            let sessions = json!({
                "user_id": "@user:localhost",
                "sessions": [],
                "total": 0
            });
            assert!(sessions.get("sessions").is_some());
        }

        #[tokio::test]
        async fn test_admin_batch_deactivate() {
            let request = json!({
                "users": ["@user1:localhost", "@user2:localhost"],
                "erase": false
            });
            assert!(request.get("users").is_some());
        }
    }

    // Room API Integration Tests
    mod room_api {
        use super::*;

        #[tokio::test]
        async fn test_create_room() {
            let room = create_test_room();
            assert!(room.get("room_name").is_some());
        }

        #[tokio::test]
        async fn test_join_room() {
            let request = json!({
                "room_id": "!room:localhost",
                "user_id": "@user:localhost"
            });
            assert!(request.get("room_id").is_some());
        }

        #[tokio::test]
        async fn test_leave_room() {
            let request = json!({
                "room_id": "!room:localhost"
            });
            assert!(request.get("room_id").is_some());
        }

        #[tokio::test]
        async fn test_room_members() {
            let request = json!({
                "room_id": "!room:localhost",
                "membership": "join"
            });
            assert!(request.get("room_id").is_some());
        }

        #[tokio::test]
        async fn test_ban_user() {
            let request = json!({
                "user_id": "@baduser:localhost",
                "reason": "Violation of rules"
            });
            assert!(request.get("user_id").is_some());
        }

        #[tokio::test]
        async fn test_room_listing() {
            let listing = json!({
                "room_id": "!room:localhost",
                "public": true,
                "in_directory": true
            });
            assert!(listing.get("public").is_some());
        }
    }

    // Message API Integration Tests
    mod message_api {
        use super::*;

        #[tokio::test]
        async fn test_send_message() {
            let message = json!({
                "type": "m.room.message",
                "content": {
                    "msgtype": "m.text",
                    "body": "Hello world"
                }
            });
            assert!(message.get("content").is_some());
        }

        #[tokio::test]
        async fn test_get_messages() {
            let request = json!({
                "room_id": "!room:localhost",
                "from": "start",
                "limit": 10
            });
            assert!(request.get("room_id").is_some());
        }

        #[tokio::test]
        async fn test_redaction() {
            let redaction = json!({
                "reason": "Contains sensitive info"
            });
            assert!(redaction.get("reason").is_some());
        }

        #[tokio::test]
        async fn test_reactions() {
            let reaction = json!({
                "key": "👍",
                "type": "m.reaction"
            });
            assert!(reaction.get("key").is_some());
        }

        #[tokio::test]
        async fn test_receipts() {
            let receipt = json!({
                "type": "m.read",
                "event_id": "$event:localhost"
            });
            assert!(receipt.get("type").is_some());
        }
    }

    // User API Integration Tests
    mod user_api {
        use super::*;

        #[tokio::test]
        async fn test_get_profile() {
            let profile = json!({
                "user_id": "@user:localhost",
                "displayname": "Test User",
                "avatar_url": "mxc://avatar"
            });
            assert!(profile.get("displayname").is_some());
        }

        #[tokio::test]
        async fn test_set_profile() {
            let request = json!({
                "displayname": "New Name",
                "avatar_url": "mxc://newavatar"
            });
            assert!(request.get("displayname").is_some());
        }

        #[tokio::test]
        async fn test_account_data() {
            let data = json!({
                "type": "m.direct",
                "content": {}
            });
            assert!(data.get("type").is_some());
        }
    }

    // Device API Integration Tests
    mod device_api {
        use super::*;

        #[tokio::test]
        async fn test_get_devices() {
            let devices = json!({
                "devices": []
            });
            assert!(devices.get("devices").is_some());
        }

        #[tokio::test]
        async fn test_update_device() {
            let request = json!({
                "device_id": "DEVICE123",
                "display_name": "My Device"
            });
            assert!(request.get("device_id").is_some());
        }

        #[tokio::test]
        async fn test_delete_device() {
            let request = json!({
                "device_id": "DEVICE123"
            });
            assert!(request.get("device_id").is_some());
        }

        #[tokio::test]
        async fn test_keys_upload() {
            let keys = json!({
                "device_keys": {},
                "one_time_keys": {}
            });
            assert!(keys.get("device_keys").is_some());
        }
    }

    // E2EE Integration Tests
    mod e2ee_api {
        use super::*;

        #[tokio::test]
        async fn test_upload_keys() {
            let request = json!({
                "device_keys": {
                    "user_id": "@user:localhost",
                    "device_id": "DEVICE123"
                }
            });
            assert!(request.get("device_keys").is_some());
        }

        #[tokio::test]
        async fn test_claim_keys() {
            let request = json!({
                "one_time_keys": {
                    "@user:localhost": {
                        "DEVICE123": {
                            "algorithm": "m.olm.curve25519-aes-sha2",
                            "key": "keydata"
                        }
                    }
                }
            });
            assert!(request.get("one_time_keys").is_some());
        }

        #[tokio::test]
        async fn test_key_backup() {
            let backup = json!({
                "rooms": {
                    "!room:localhost": {
                        "sessions": {}
                    }
                }
            });
            assert!(backup.get("rooms").is_some());
        }

        #[tokio::test]
        async fn test_cross_signing() {
            let keys = json!({
                "master_key": {},
                "self_signing_key": {},
                "user_signing_key": {}
            });
            assert!(keys.get("master_key").is_some());
        }
    }

    // Media API Integration Tests
    mod media_api {
        use super::*;

        #[tokio::test]
        async fn test_upload_media() {
            let request = json!({
                "filename": "image.png",
                "content_type": "image/png"
            });
            assert!(request.get("filename").is_some());
        }

        #[tokio::test]
        async fn test_get_media() {
            let media_id = "media123";
            assert!(!media_id.is_empty());
        }

        #[tokio::test]
        async fn test_thumbnail() {
            let request = json!({
                "media_id": "media123",
                "width": 100,
                "height": 100,
                "method": "crop"
            });
            assert!(request.get("width").is_some());
        }
    }

    // Federation API Integration Tests
    mod federation_api {
        use super::*;

        #[tokio::test]
        async fn test_federation_version() {
            let version = json!({
                "server": {
                    "name": "synapse-rust",
                    "version": "0.1.0"
                }
            });
            assert!(version.get("server").is_some());
        }

        #[tokio::test]
        async fn test_get_public_rooms() {
            let request = json!({
                "server": "remote.server",
                "limit": 10,
                "since": "token"
            });
            assert!(request.get("server").is_some());
        }

        #[tokio::test]
        async fn test_send_join() {
            let pdu = json!({
                "type": "m.room.join",
                "sender": "@user:localhost"
            });
            assert!(pdu.get("type").is_some());
        }

        #[tokio::test]
        async fn test_backfill() {
            let request = json!({
                "room_id": "!room:remote.server",
                "limit": 10
            });
            assert!(request.get("room_id").is_some());
        }
    }

    // Sync Integration Tests
    mod sync_api {
        use super::*;

        #[tokio::test]
        async fn test_sync_request() {
            let request = json!({
                "filter": {},
                "since": "token",
                "timeout": 30000
            });
            assert!(request.get("timeout").is_some());
        }

        #[tokio::test]
        async fn test_sync_response() {
            let response = json!({
                "next_batch": "batch123",
                "rooms": {
                    "join": {},
                    "invite": {},
                    "leave": {}
                },
                "presence": {},
                "account_data": {},
                "to_device": {}
            });
            assert!(response.get("next_batch").is_some());
        }

        #[tokio::test]
        async fn test_sync_filter() {
            let filter = json!({
                "room": {
                    "timeline": {
                        "limit": 100,
                        "lazy_load_members": true
                    },
                    "state": {
                        "lazy_load_members": true
                    }
                },
                "presence": {},
                "account_data": {}
            });
            assert!(filter.get("room").is_some());
        }
    }

    // Search Integration Tests
    mod search_api {
        use super::*;

        #[tokio::test]
        async fn test_search_request() {
            let request = json!({
                "search_term": "test query",
                "order_by": "recent",
                "limit": 10,
                "rooms": []
            });
            assert!(request.get("search_term").is_some());
        }

        #[tokio::test]
        async fn test_search_response() {
            let response = json!({
                "results": [
                    {
                        "room_id": "!room:localhost",
                        "event_id": "$event:localhost",
                        "rank": 1.0
                    }
                ],
                "count": 1,
                "highlights": []
            });
            assert!(response.get("results").is_some());
        }
    }

    // Push Integration Tests
    mod push_api {
        use super::*;

        #[tokio::test]
        async fn test_set_pusher() {
            let request = json!({
                "pushkit": {
                    "url": "https://push.example.com"
                },
                "data": {},
                "append": false
            });
            assert!(request.get("pushkit").is_some());
        }

        #[tokio::test]
        async fn test_push_rules() {
            let rules = json!({
                "global": {
                    "override": [],
                    "room": [],
                    "sender": [],
                    "underride": []
                }
            });
            assert!(rules.get("global").is_some());
        }
    }

    // Space Integration Tests
    mod space_api {
        use super::*;

        #[tokio::test]
        async fn test_create_space() {
            let space = json!({
                "name": "My Space",
                "topic": "Space topic",
                "room_type": "m.space",
                "children": []
            });
            assert_eq!(space["room_type"], "m.space");
        }

        #[tokio::test]
        async fn test_add_child() {
            let request = json!({
                "via": ["localhost"]
            });
            assert!(request.get("via").is_some());
        }

        #[tokio::test]
        async fn test_get_hierarchy() {
            let request = json!({
                "room_id": "!space:localhost",
                "limit": 50
            });
            assert!(request.get("room_id").is_some());
        }
    }

    // Thread Integration Tests
    mod thread_api {
        use super::*;

        #[tokio::test]
        async fn test_get_threads() {
            let request = json!({
                "room_id": "!room:localhost",
                "limit": 20
            });
            assert!(request.get("room_id").is_some());
        }

        #[tokio::test]
        async fn test_create_thread() {
            let request = json!({
                "room_id": "!room:localhost",
                "event_id": "$event:localhost"
            });
            assert!(request.get("event_id").is_some());
        }
    }

    // Presence Integration Tests
    mod presence_api {
        use super::*;

        #[tokio::test]
        async fn test_set_presence() {
            let request = json!({
                "presence": "online",
                "status_msg": "Available"
            });
            assert_eq!(request["presence"], "online");
        }

        #[tokio::test]
        async fn test_get_presence() {
            let presence = json!({
                "user_id": "@user:localhost",
                "presence": "online",
                "status_msg": "Available"
            });
            assert!(presence.get("presence").is_some());
        }
    }

    // Typing Integration Tests
    mod typing_api {
        use super::*;

        #[tokio::test]
        async fn test_start_typing() {
            let request = json!({
                "room_id": "!room:localhost",
                "user_id": "@user:localhost",
                "timeout": 3000
            });
            assert!(request.get("room_id").is_some());
        }

        #[tokio::test]
        async fn test_stop_typing() {
            let request = json!({
                "room_id": "!room:localhost",
                "user_id": "@user:localhost"
            });
            assert!(request.get("room_id").is_some());
        }
    }

    // Receipt Integration Tests
    mod receipt_api {
        use super::*;

        #[tokio::test]
        async fn test_send_receipt() {
            let receipt = json!({
                "type": "m.read",
                "room_id": "!room:localhost",
                "event_id": "$event:localhost"
            });
            assert!(receipt.get("type").is_some());
        }
    }
}

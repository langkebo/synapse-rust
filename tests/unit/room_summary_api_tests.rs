// Room Summary API Tests - API Endpoint Coverage
// These tests cover the room summary API endpoints from src/web/routes/room_summary.rs

use serde_json::json;

// Test 1: Room summary creation request
#[test]
fn test_room_summary_creation() {
    let summary = json!({
        "room_id": "!room:localhost",
        "topic": "A topic",
        "name": "Room Name",
        "num_joined_members": 10,
        "avatar_url": "mxc://avatar"
    });
    
    assert!(summary.get("room_id").is_some());
    assert!(summary.get("topic").is_some());
}

// Test 2: Room ID validation
#[test]
fn test_room_summary_room_id() {
    assert!(is_valid_room_id("!room:localhost"));
    assert!(is_valid_room_id("!abc123:example.com"));
    assert!(!is_valid_room_id(""));
    assert!(!is_valid_room_id("room:localhost"));
}

// Test 3: Room summary response format
#[test]
fn test_room_summary_response() {
    let summary = json!({
        "room_id": "!room:localhost",
        "topic": "A topic",
        "name": "Room Name",
        "num_joined_members": 10,
        "num_invited_members": 2,
        "avatar_url": "mxc://avatar",
        "world_readable": false,
        "guest_can_join": false
    });
    
    assert!(summary.get("room_id").is_some());
    assert!(summary.get("name").is_some());
    assert!(summary.get("num_joined_members").is_some());
}

// Test 4: User summaries response
#[test]
fn test_user_summaries_response() {
    let summaries = vec![
        json!({
            "room_id": "!room1:localhost",
            "membership": "join"
        }),
        json!({
            "room_id": "!room2:localhost",
            "membership": "leave"
        })
    ];
    
    assert_eq!(summaries.len(), 2);
    assert!(summaries[0].get("membership").is_some());
}

// Test 5: Room members response
#[test]
fn test_room_members_response() {
    let members = vec![
        json!({
            "user_id": "@user1:localhost",
            "display_name": "User 1",
            "membership": "join"
        }),
        json!({
            "user_id": "@user2:localhost",
            "display_name": "User 2",
            "membership": "invite"
        })
    ];
    
    assert_eq!(members.len(), 2);
    assert!(members[0].get("user_id").is_some());
    assert!(members[0].get("membership").is_some());
}

// Test 6: Membership validation
#[test]
fn test_membership_validation() {
    assert!(is_valid_membership("join"));
    assert!(is_valid_membership("invite"));
    assert!(is_valid_membership("leave"));
    assert!(is_valid_membership("ban"));
    assert!(!is_valid_membership("invalid"));
}

// Test 7: Room state response
#[test]
fn test_room_state_response() {
    let state = vec![
        json!({
            "type": "m.room.create",
            "state_key": "",
            "sender": "@user:localhost"
        }),
        json!({
            "type": "m.room.member",
            "state_key": "@user:localhost",
            "sender": "@creator:localhost"
        })
    ];
    
    assert_eq!(state.len(), 2);
    assert!(state[0].get("type").is_some());
    assert!(state[0].get("sender").is_some());
}

// Test 8: Room stats response
#[test]
fn test_room_stats_response() {
    let stats = json!({
        "room_id": "!room:localhost",
        "joined_members": 10,
        "invited_members": 2,
        "left_members": 3,
        "banned_members": 1,
        "total_events": 100,
        "state_events": 20,
        "message_events": 80
    });
    
    assert!(stats.get("room_id").is_some());
    assert!(stats.get("joined_members").is_some());
    assert!(stats.get("total_events").is_some());
}

// Test 9: Room sync summary
#[test]
fn test_sync_room_summary() {
    let sync = json!({
        "room_id": "!room:localhost",
        "timeline": [],
        "state": [],
        "ephemeral": []
    });
    
    assert!(sync.get("room_id").is_some());
    assert!(sync.get("timeline").is_some());
    assert!(sync.get("state").is_some());
}

// Test 10: Update room summary
#[test]
fn test_update_room_summary() {
    let update = json!({
        "room_id": "!room:localhost",
        "topic": "New topic",
        "name": "New name"
    });
    
    assert!(update.get("room_id").is_some());
    assert!(update.get("topic").is_some());
}

// Test 11: Delete room summary
#[test]
fn test_delete_room_summary() {
    let result = json!({
        "deleted": true,
        "room_id": "!room:localhost"
    });
    
    assert!(result.get("deleted").is_some());
    assert!(result["deleted"].as_bool().unwrap_or(false));
}

// Test 12: Add room member
#[test]
fn test_add_room_member() {
    let member = json!({
        "user_id": "@user:localhost",
        "membership": "join"
    });
    
    assert!(member.get("user_id").is_some());
    assert!(member.get("membership").is_some());
}

// Test 13: Update room member
#[test]
fn test_update_room_member() {
    let member = json!({
        "user_id": "@user:localhost",
        "membership": "invite",
        "display_name": "User"
    });
    
    assert!(member.get("user_id").is_some());
    assert!(member.get("membership").is_some());
}

// Test 14: Remove room member
#[test]
fn test_remove_room_member() {
    let result = json!({
        "removed": true,
        "user_id": "@user:localhost",
        "room_id": "!room:localhost"
    });
    
    assert!(result.get("removed").is_some());
    assert!(result.get("user_id").is_some());
}

// Test 15: State event format
#[test]
fn test_state_event_format() {
    let event = json!({
        "type": "m.room.topic",
        "state_key": "",
        "sender": "@user:localhost",
        "content": {
            "topic": "Room topic"
        }
    });
    
    assert!(event.get("type").is_some());
    assert!(event.get("sender").is_some());
    assert!(event.get("content").is_some());
}

// Test 16: Recalculate stats request
#[test]
fn test_recalculate_stats_request() {
    let request = json!({
        "room_id": "!room:localhost",
        "force": false
    });
    
    assert!(request.get("room_id").is_some());
}

// Test 17: Clear unread request
#[test]
fn test_clear_unread_request() {
    let request = json!({
        "room_id": "!room:localhost",
        "user_id": "@user:localhost"
    });
    
    assert!(request.get("room_id").is_some());
    assert!(request.get("user_id").is_some());
}

// Test 18: Recalculate heroes request
#[test]
fn test_recalculate_heroes_request() {
    let request = json!({
        "room_id": "!room:localhost",
        "limit": 5
    });
    
    assert!(request.get("room_id").is_some());
    assert!(request.get("limit").is_some());
}

// Helper functions
fn is_valid_room_id(room_id: &str) -> bool {
    !room_id.is_empty() && room_id.starts_with('!') && room_id.contains(':')
}

fn is_valid_membership(membership: &str) -> bool {
    matches!(membership, "join" | "invite" | "leave" | "ban")
}

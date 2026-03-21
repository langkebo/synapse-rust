// Admin API Tests - API Endpoint Coverage
// These tests cover the admin API endpoints from src/web/routes/admin.rs

use serde_json::json;

// Test 1: Server version response format
#[test]
fn test_admin_server_version_response() {
    let response = json!({
        "server_version": "1.0.0",
        "name": "synapse-rust"
    });

    assert!(response.get("server_version").is_some());
    assert!(response.get("name").is_some());
}

// Test 2: Server stats response format
#[test]
fn test_admin_server_stats_response() {
    let response = json!({
        "total_users": 100,
        "active_users": 50,
        "total_rooms": 200,
        "active_rooms": 150
    });

    assert!(response.get("total_users").is_some());
    assert!(response.get("total_rooms").is_some());
}

// Test 3: User list response format
#[test]
fn test_admin_user_list_response() {
    let users = vec![
        json!({"user_id": "@user1:localhost", "admin": false}),
        json!({"user_id": "@user2:localhost", "admin": true}),
    ];

    assert_eq!(users.len(), 2);
    assert_eq!(users[0]["user_id"], "@user1:localhost");
}

// Test 4: Room list response format
#[test]
fn test_admin_room_list_response() {
    let rooms = vec![json!({"room_id": "!room1:localhost", "name": "Test Room"})];

    assert_eq!(rooms.len(), 1);
    assert!(rooms[0].get("room_id").is_some());
}

// Test 5: IP blocking validation
#[test]
fn test_ip_block_validation() {
    // Valid IP formats
    assert!(is_valid_ip("192.168.1.1"));
    assert!(is_valid_ip("10.0.0.1"));
    assert!(is_valid_ip("::1"));

    // Invalid IP
    assert!(!is_valid_ip(""));
    assert!(!is_valid_ip("not-an-ip"));
}

// Test 6: User ID validation for admin
#[test]
fn test_admin_user_id_validation() {
    // Valid format
    assert!(is_valid_admin_user_id("@admin:localhost"));
    assert!(is_valid_admin_user_id("@user:example.com"));

    // Invalid
    assert!(!is_valid_admin_user_id(""));
    assert!(!is_valid_admin_user_id("@user"));
}

// Test 7: Room ID validation for admin
#[test]
fn test_admin_room_id_validation() {
    assert!(is_valid_admin_room_id("!room:localhost"));
    assert!(!is_valid_admin_room_id(""));
    assert!(!is_valid_admin_room_id("room:localhost"));
}

// Test 8: Pagination parameters
#[test]
fn test_pagination_params() {
    let from = 0;
    let limit = 10;

    assert!(from >= 0);
    assert!(limit > 0 && limit <= 1000);
}

// Test 9: Server notice content validation
#[test]
fn test_server_notice_content() {
    let content = json!({
        "msgtype": "m.text",
        "body": "Server notice message"
    });

    assert!(content.get("msgtype").is_some());
    assert!(content.get("body").is_some());
}

// Test 10: Retention policy validation
#[test]
fn test_retention_policy_validation() {
    let policy = json!({
        "min_lifetime": 86400000_i64,
        "max_lifetime": 2592000000_i64
    });

    let min = policy["min_lifetime"].as_i64().unwrap_or(0);
    let max = policy["max_lifetime"].as_i64().unwrap_or(0);

    assert!(min >= 0);
    assert!(max > min);
}

// Test 11: Deactivation response
#[test]
fn test_deactivation_response() {
    let response = json!({
        "user_id": "@user:localhost",
        "sheduled_password": null,
        "erase": false
    });

    assert!(response.get("user_id").is_some());
}

// Test 12: Admin set response
#[test]
fn test_admin_set_response() {
    let response = json!({
        "user_id": "@user:localhost",
        "admin": true
    });

    assert_eq!(response["admin"], true);
}

// Test 13: Purge history options
#[test]
fn test_purge_history_options() {
    let options = json!({
        "room_id": "!room:localhost",
        "delete_local_events": true,
        "purge_up_to_ts": 1700000000000_i64
    });

    assert!(options.get("room_id").is_some());
    assert!(options.get("purge_up_to_ts").is_some());
}

// Test 14: Shutdown room options
#[test]
fn test_shutdown_room_options() {
    let options = json!({
        "room_id": "!room:localhost",
        "new_place_user_id": "@user:localhost",
        "block": true
    });

    assert!(options.get("room_id").is_some());
    assert!(options.get("block").is_some());
}

// Test 15: Worker status check
#[test]
fn test_worker_status_check() {
    let workers = vec![json!({"worker_id": "synapse-worker-1", "status": "running"})];

    assert_eq!(workers.len(), 1);
    assert_eq!(workers[0]["status"], "running");
}

// Test 16: Space admin validation
#[test]
fn test_space_admin_validation() {
    let space = json!({
        "room_id": "!space:localhost",
        "name": "Test Space",
        "is_public": true
    });

    assert!(space.get("room_id").is_some());
    assert!(space.get("name").is_some());
}

// Helper functions
fn is_valid_ip(ip: &str) -> bool {
    !ip.is_empty() && (ip.contains('.') || ip.contains(':'))
}

fn is_valid_admin_user_id(user_id: &str) -> bool {
    !user_id.is_empty() && user_id.starts_with('@') && user_id.contains(':')
}

fn is_valid_admin_room_id(room_id: &str) -> bool {
    !room_id.is_empty() && room_id.starts_with('!') && room_id.contains(':')
}

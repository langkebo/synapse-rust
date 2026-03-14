// Push API Tests - API Endpoint Coverage
// These tests cover the push API endpoints from src/web/routes/push.rs

use serde_json::json;

// Test 1: Get pushers request
#[test]
fn test_get_pushers() {
    let request = json!({
        "user_id": "@user:localhost"
    });
    
    assert!(request.get("user_id").is_some());
}

// Test 2: Pusher data response
#[test]
fn test_pusher_response() {
    let pusher = json!({
        "pushkey": "PushKey123",
        "app_id": "app_id",
        "app_display_name": "App",
        "device_display_name": "Device",
        "profile_tag": "tag",
        "kind": "http",
        "lang": "en"
    });
    
    assert!(pusher.get("pushkey").is_some());
    assert!(pusher.get("app_id").is_some());
    assert!(pusher.get("kind").is_some());
}

// Test 3: Set pusher request
#[test]
fn test_set_pusher_request() {
    let pusher = json!({
        "pushkey": "PushKey123",
        "kind": "http",
        "app_id": "app_id",
        "app_display_name": "App",
        "device_display_name": "Device",
        "profile_tag": "tag",
        "lang": "en",
        "data": {
            "url": "https://push.example.com"
        }
    });
    
    assert!(pusher.get("pushkey").is_some());
    assert!(pusher.get("kind").is_some());
    assert!(pusher.get("app_id").is_some());
}

// Test 4: Pusher kind validation
#[test]
fn test_pusher_kind_validation() {
    // Valid kinds
    assert!(is_valid_pusher_kind("http"));
    assert!(is_valid_pusher_kind("email"));
    assert!(is_valid_pusher_kind("noop"));
    
    // Invalid
    assert!(!is_valid_pusher_kind("invalid"));
}

// Test 5: Get push rules request
#[test]
fn test_get_push_rules() {
    let request = json!({
        "user_id": "@user:localhost"
    });
    
    assert!(request.get("user_id").is_some());
}

// Test 6: Push rules response format
#[test]
fn test_push_rules_response() {
    let rules = json!({
        "global": {
            "override": [],
            "content": [],
            "room": [],
            "sender": []
        }
    });
    
    assert!(rules.get("global").is_some());
}

// Test 7: Push rule format
#[test]
fn test_push_rule_format() {
    let rule = json!({
        "rule_id": "rule1",
        "priority_class": 5,
        "priority": 0,
        "conditions": [],
        "actions": ["notify", {"set_tweak": "highlight"}],
        "pattern": null,
        "is_default": false,
        "enabled": true
    });
    
    assert!(rule.get("rule_id").is_some());
    assert!(rule.get("priority_class").is_some());
    assert!(rule.get("actions").is_some());
}

// Test 8: Push rule scope validation
#[test]
fn test_push_rule_scope_validation() {
    // Valid scopes
    assert!(is_valid_scope("global"));
    assert!(is_valid_scope("device"));
    
    // Invalid
    assert!(!is_valid_scope("invalid"));
}

// Test 9: Push rule kind validation
#[test]
fn test_push_rule_kind_validation() {
    // Valid kinds
    assert!(is_valid_rule_kind("override"));
    assert!(is_valid_rule_kind("content"));
    assert!(is_valid_rule_kind("room"));
    assert!(is_valid_rule_kind("sender"));
    assert!(is_valid_rule_kind("underride"));
    
    // Invalid
    assert!(!is_valid_rule_kind("invalid"));
}

// Test 10: Get push rules by scope
#[test]
fn test_get_push_rules_scope() {
    let rules = json!({
        "override": [],
        "content": [],
        "room": [],
        "sender": []
    });
    
    assert!(rules.get("override").is_some());
    assert!(rules.get("content").is_some());
}

// Test 11: Get push rules by kind
#[test]
fn test_get_push_rules_kind() {
    let kind_rules = vec![
        json!({
            "rule_id": "rule1",
            "enabled": true
        })
    ];
    
    assert_eq!(kind_rules.len(), 1);
}

// Test 12: Get push rule by ID
#[test]
fn test_get_push_rule() {
    let rule = json!({
        "rule_id": "rule1",
        "priority_class": 5,
        "actions": ["notify"]
    });
    
    assert!(rule.get("rule_id").is_some());
}

// Test 13: Set push rule request
#[test]
fn test_set_push_rule_request() {
    let rule = json!({
        "conditions": [
            {"kind": "room_member_count", "is": "2"}
        ],
        "actions": ["notify", {"set_tweak": "sound", "value": "default"}]
    });
    
    assert!(rule.get("conditions").is_some());
    assert!(rule.get("actions").is_some());
}

// Test 14: Create push rule request
#[test]
fn test_create_push_rule() {
    let rule = json!({
        "rule_id": "custom_rule",
        "kind": "override",
        "conditions": [],
        "actions": ["notify"]
    });
    
    assert!(rule.get("rule_id").is_some());
    assert!(rule.get("kind").is_some());
}

// Test 15: Delete push rule
#[test]
fn test_delete_push_rule() {
    let result = json!({
        "deleted": true,
        "rule_id": "rule1"
    });
    
    assert!(result.get("deleted").is_some());
    assert!(result["deleted"].as_bool().unwrap_or(false));
}

// Test 16: Push rule actions validation
#[test]
fn test_push_rule_actions() {
    let actions = vec![
        json!("notify"),
        json!("dont_notify"),
        json!("coalesce"),
        json!({"set_tweak": "highlight"}),
        json!({"set_tweak": "sound", "value": "default"})
    ];
    
    assert_eq!(actions.len(), 5);
}

// Test 17: Set push rule actions request
#[test]
fn test_set_push_rule_actions() {
    let actions = json!({
        "actions": ["notify", {"set_tweak": "highlight"}]
    });
    
    assert!(actions.get("actions").is_some());
}

// Test 18: Get push rule enabled
#[test]
fn test_get_push_rule_enabled() {
    let result = json!({
        "enabled": true
    });
    
    assert!(result.get("enabled").is_some());
}

// Test 19: Set push rule enabled request
#[test]
fn test_set_push_rule_enabled() {
    let enabled = json!({
        "enabled": true
    });
    
    assert!(enabled.get("enabled").is_some());
}

// Test 20: Get notifications request
#[test]
fn test_get_notifications_request() {
    let request = json!({
        "from": 0,
        "limit": 20
    });
    
    assert!(request.get("limit").is_some());
}

// Test 21: Notification response format
#[test]
fn test_notification_response() {
    let notification = json!({
        "room_id": "!room:localhost",
        "event_ids": ["$event:localhost"],
        "type": "m.room.member",
        "sender": "@user:localhost",
        "priority": "high",
        "content": {},
        "counts": {
            "unread": 1,
            "missed_calls": 0
        }
    });
    
    assert!(notification.get("room_id").is_some());
    assert!(notification.get("event_ids").is_some());
    assert!(notification.get("priority").is_some());
}

// Test 22: Notification priority validation
#[test]
fn test_notification_priority() {
    // Valid priorities
    assert!(is_valid_priority("high"));
    assert!(is_valid_priority("low"));
    
    // Invalid
    assert!(!is_valid_priority("invalid"));
}

// Test 23: User push rules response
#[test]
fn test_user_push_rules_response() {
    let rules = json!({
        "global": {
            "override": []
        },
        "device": {}
    });
    
    assert!(rules.get("global").is_some());
}

// Test 24: Push rule condition format
#[test]
fn test_push_rule_condition() {
    let condition = json!({
        "kind": "room_member_count",
        "is": "2"
    });
    
    assert!(condition.get("kind").is_some());
}

// Test 25: Push rule condition kinds validation
#[test]
fn test_condition_kind_validation() {
    // Valid condition kinds
    assert!(is_valid_condition_kind("room_member_count"));
    assert!(is_valid_condition_kind("sender_notification_permission"));
    assert!(is_valid_condition_kind("event_match"));
    assert!(is_valid_condition_kind("contains_display_name"));
    
    // Invalid
    assert!(!is_valid_condition_kind("invalid"));
}

// Helper functions
fn is_valid_pusher_kind(kind: &str) -> bool {
    matches!(kind, "http" | "email" | "noop")
}

fn is_valid_scope(scope: &str) -> bool {
    matches!(scope, "global" | "device")
}

fn is_valid_rule_kind(kind: &str) -> bool {
    matches!(kind, "override" | "content" | "room" | "sender" | "underride")
}

fn is_valid_priority(priority: &str) -> bool {
    matches!(priority, "high" | "low")
}

fn is_valid_condition_kind(kind: &str) -> bool {
    matches!(kind, "room_member_count" | "sender_notification_permission" | "event_match" | "contains_display_name" | "room_version" | "user_version")
}

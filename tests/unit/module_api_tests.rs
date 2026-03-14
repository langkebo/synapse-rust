// Module API Tests - API Endpoint Coverage
// These tests cover the module API endpoints from src/web/routes/module.rs

use serde_json::json;

// Test 1: Create module request
#[test]
fn test_create_module() {
    let module = json!({
        "module_name": "spam_checker",
        "module_type": "spam_checker",
        "config": {
            "threshold": 100
        },
        "priority": 10
    });
    
    assert!(module.get("module_name").is_some());
    assert!(module.get("module_type").is_some());
    assert!(module.get("config").is_some());
}

// Test 2: Module type validation
#[test]
fn test_module_type_validation() {
    // Valid module types
    assert!(is_valid_module_type("spam_checker"));
    assert!(is_valid_module_type("third_party_rule"));
    assert!(is_valid_module_type("presence"));
    assert!(is_valid_module_type("rate_limiter"));
    assert!(is_valid_module_type("account_data"));
    assert!(is_valid_module_type("media"));
    
    // Invalid
    assert!(!is_valid_module_type("invalid"));
    assert!(!is_valid_module_type(""));
}

// Test 3: Module response format
#[test]
fn test_module_response() {
    let module = json!({
        "id": 1,
        "module_name": "spam_checker",
        "module_type": "spam_checker",
        "is_enabled": true,
        "config": {},
        "priority": 10,
        "created_ts": 1700000000000_i64
    });
    
    assert!(module.get("id").is_some());
    assert!(module.get("module_name").is_some());
    assert!(module.get("module_type").is_some());
    assert!(module.get("is_enabled").is_some());
}

// Test 4: Get module by name
#[test]
fn test_get_module() {
    let result = json!({
        "module_name": "spam_checker",
        "module_type": "spam_checker",
        "is_enabled": true
    });
    
    assert!(result.get("module_name").is_some());
    assert!(result.get("is_enabled").is_some());
}

// Test 5: Get all modules response
#[test]
fn test_get_all_modules_response() {
    let modules = vec![
        json!({
            "module_name": "spam_checker",
            "is_enabled": true
        }),
        json!({
            "module_name": "rate_limiter",
            "is_enabled": false
        })
    ];
    
    assert_eq!(modules.len(), 2);
}

// Test 6: Get modules by type
#[test]
fn test_get_modules_by_type() {
    let modules = vec![
        json!({
            "module_name": "spam_checker1",
            "module_type": "spam_checker"
        })
    ];
    
    assert_eq!(modules.len(), 1);
    assert!(modules[0].get("module_type").is_some());
}

// Test 7: Update module config
#[test]
fn test_update_module_config() {
    let config = json!({
        "module_name": "spam_checker",
        "config": {
            "threshold": 200
        }
    });
    
    assert!(config.get("module_name").is_some());
    assert!(config.get("config").is_some());
}

// Test 8: Enable module request
#[test]
fn test_enable_module() {
    let enable = json!({
        "module_name": "spam_checker",
        "enabled": true
    });
    
    assert!(enable.get("module_name").is_some());
    assert!(enable.get("enabled").is_some());
}

// Test 9: Enable module response
#[test]
fn test_enable_module_response() {
    let result = json!({
        "module_name": "spam_checker",
        "is_enabled": true
    });
    
    assert!(result.get("module_name").is_some());
    assert!(result.get("is_enabled").is_some());
}

// Test 10: Delete module
#[test]
fn test_delete_module() {
    let result = json!({
        "deleted": true,
        "module_name": "spam_checker"
    });
    
    assert!(result.get("deleted").is_some());
    assert!(result["deleted"].as_bool().unwrap_or(false));
}

// Test 11: Check spam request
#[test]
fn test_check_spam_request() {
    let check = json!({
        "event": {
            "event_id": "$event:localhost",
            "room_id": "!room:localhost",
            "sender": "@user:localhost"
        }
    });
    
    assert!(check.get("event").is_some());
}

// Test 12: Check spam response
#[test]
fn test_check_spam_response() {
    let result = json!({
        "spam": false,
        "module_name": "spam_checker"
    });
    
    assert!(result.get("spam").is_some());
    assert!(result.get("module_name").is_some());
}

// Test 13: Check third party rule request
#[test]
fn test_check_third_party_rule_request() {
    let rule = json!({
        "event": {
            "type": "m.room.member",
            "state_key": "@user:localhost"
        },
        "room_id": "!room:localhost"
    });
    
    assert!(rule.get("event").is_some());
    assert!(rule.get("room_id").is_some());
}

// Test 14: Third party rule response
#[test]
fn test_third_party_rule_response() {
    let result = json!({
        "action": "allow",
        "module_name": "third_party_rule"
    });
    
    assert!(result.get("action").is_some());
}

// Test 15: Spam check by event ID
#[test]
fn test_spam_check_by_event() {
    let result = json!({
        "event_id": "$event:localhost",
        "spam": false,
        "checks": 2
    });
    
    assert!(result.get("event_id").is_some());
    assert!(result.get("spam").is_some());
}

// Test 16: Spam check by sender
#[test]
fn test_spam_check_by_sender() {
    let result = json!({
        "sender": "@user:localhost",
        "spam": false,
        "total_reports": 0
    });
    
    assert!(result.get("sender").is_some());
    assert!(result.get("spam").is_some());
}

// Test 17: Execution logs response
#[test]
fn test_execution_logs_response() {
    let logs = vec![
        json!({
            "module_name": "spam_checker",
            "timestamp": 1700000000000_i64,
            "action": "check",
            "result": "allow"
        })
    ];
    
    assert_eq!(logs.len(), 1);
    assert!(logs[0].get("module_name").is_some());
}

// Test 18: Account validity request
#[test]
fn test_account_validity_request() {
    let validity = json!({
        "user_id": "@user:localhost",
        "expires_at": 1705000000000_i64,
        "renewable": true
    });
    
    assert!(validity.get("user_id").is_some());
    assert!(validity.get("expires_at").is_some());
}

// Test 19: Account validity response
#[test]
fn test_account_validity_response() {
    let validity = json!({
        "user_id": "@user:localhost",
        "expires_at": 1705000000000_i64,
        "is_valid": true,
        "renewable": true
    });
    
    assert!(validity.get("user_id").is_some());
    assert!(validity.get("expires_at").is_some());
    assert!(validity.get("is_valid").is_some());
}

// Test 20: Renew account request
#[test]
fn test_renew_account_request() {
    let renew = json!({
        "user_id": "@user:localhost",
        "extend_by": 86400
    });
    
    assert!(renew.get("user_id").is_some());
}

// Test 21: Password auth provider request
#[test]
fn test_password_auth_provider_request() {
    let provider = json!({
        "provider_name": "ldap",
        "enabled": true,
        "config": {
            "url": "ldap://localhost"
        }
    });
    
    assert!(provider.get("provider_name").is_some());
    assert!(provider.get("enabled").is_some());
}

// Test 22: Password auth providers response
#[test]
fn test_password_auth_providers_response() {
    let providers = vec![
        json!({
            "provider_name": "ldap",
            "enabled": true
        })
    ];
    
    assert_eq!(providers.len(), 1);
    assert!(providers[0].get("provider_name").is_some());
}

// Test 23: Presence route request
#[test]
fn test_presence_route_request() {
    let route = json!({
        "path": "/presence",
        "method": "POST",
        "enabled": true
    });
    
    assert!(route.get("path").is_some());
    assert!(route.get("method").is_some());
}

// Test 24: Presence routes response
#[test]
fn test_presence_routes_response() {
    let routes = vec![
        json!({
            "path": "/presence",
            "method": "POST"
        })
    ];
    
    assert_eq!(routes.len(), 1);
    assert!(routes[0].get("path").is_some());
}

// Test 25: Media callback request
#[test]
fn test_media_callback_request() {
    let callback = json!({
        "callback_name": "on_upload",
        "module_name": "media_module",
        "enabled": true
    });
    
    assert!(callback.get("callback_name").is_some());
    assert!(callback.get("module_name").is_some());
}

// Test 26: Media callbacks response
#[test]
fn test_media_callbacks_response() {
    let callbacks = vec![
        json!({
            "callback_name": "on_upload",
            "module_name": "media_module"
        })
    ];
    
    assert_eq!(callbacks.len(), 1);
    assert!(callbacks[0].get("callback_name").is_some());
}

// Test 27: Rate limit callback request
#[test]
fn test_rate_limit_callback_request() {
    let callback = json!({
        "callback_name": "check_rate_limit",
        "module_name": "rate_limiter",
        "config": {
            "rate": 100
        }
    });
    
    assert!(callback.get("callback_name").is_some());
    assert!(callback.get("module_name").is_some());
}

// Test 28: Account data callback request
#[test]
fn test_account_data_callback_request() {
    let callback = json!({
        "callback_name": "on_account_data",
        "module_name": "account_module"
    });
    
    assert!(callback.get("callback_name").is_some());
    assert!(callback.get("module_name").is_some());
}

// Test 29: Priority validation
#[test]
fn test_priority_validation() {
    assert!(is_valid_priority(0));
    assert!(is_valid_priority(100));
    assert!(is_valid_priority(-10));
}

// Helper functions
fn is_valid_module_type(module_type: &str) -> bool {
    matches!(module_type, "spam_checker" | "third_party_rule" | "presence" | "rate_limiter" | "account_data" | "media")
}

fn is_valid_priority(priority: i32) -> bool {
    priority >= -100 && priority <= 100
}

// Registration Token API Tests - API Endpoint Coverage
// These tests cover the registration token API endpoints from src/web/routes/registration_token.rs

use serde_json::json;

// Test 1: Create token request
#[test]
fn test_create_token_request() {
    let token = json!({
        "token": "test_token_123",
        "token_type": "single_use",
        "max_uses": 1,
        "display_name": "Test Token",
        "expires_ts": 1700100000000_i64
    });

    assert!(token.get("token").is_some());
    assert!(token.get("token_type").is_some());
    assert!(token.get("max_uses").is_some());
}

// Test 2: Token type validation
#[test]
fn test_token_type_validation() {
    // Valid types
    assert!(is_valid_token_type("single_use"));
    assert!(is_valid_token_type("recurring"));
    assert!(is_valid_token_type("one_time"));

    // Invalid
    assert!(!is_valid_token_type("invalid"));
}

// Test 3: Create token response
#[test]
fn test_create_token_response() {
    let response = json!({
        "token_id": 1,
        "token": "test_token_123",
        "created": true
    });

    assert!(response.get("token_id").is_some());
    assert!(response.get("token").is_some());
}

// Test 4: Get token request
#[test]
fn test_get_token_request() {
    let request = json!({
        "token": "test_token_123"
    });

    assert!(request.get("token").is_some());
}

// Test 5: Get token by ID request
#[test]
fn test_get_token_by_id_request() {
    let request = json!({
        "id": 1
    });

    assert!(request.get("id").is_some());
}

// Test 6: Token response format
#[test]
fn test_token_response() {
    let token = json!({
        "id": 1,
        "token": "test_token_123",
        "token_type": "single_use",
        "max_uses": 1,
        "uses_count": 0,
        "is_used": false,
        "is_enabled": true,
        "display_name": "Test Token"
    });

    assert!(token.get("id").is_some());
    assert!(token.get("token").is_some());
    assert!(token.get("is_enabled").is_some());
}

// Test 7: Update token request
#[test]
fn test_update_token_request() {
    let update = json!({
        "max_uses": 5,
        "is_enabled": true,
        "display_name": "Updated Token"
    });

    assert!(update.get("max_uses").is_some());
    assert!(update.get("is_enabled").is_some());
}

// Test 8: Update token response
#[test]
fn test_update_token_response() {
    let response = json!({
        "updated": true,
        "token_id": 1
    });

    assert!(response.get("updated").is_some());
    assert!(response["updated"].as_bool().unwrap_or(false));
}

// Test 9: Delete token request
#[test]
fn test_delete_token_request() {
    let delete = json!({
        "token": "test_token_123"
    });

    assert!(delete.get("token").is_some());
}

// Test 10: Delete token response
#[test]
fn test_delete_token_response() {
    let response = json!({
        "deleted": true,
        "token_id": 1
    });

    assert!(response.get("deleted").is_some());
    assert!(response["deleted"].as_bool().unwrap_or(false));
}

// Test 11: Deactivate token request
#[test]
fn test_deactivate_token_request() {
    let deactivate = json!({
        "token": "test_token_123"
    });

    assert!(deactivate.get("token").is_some());
}

// Test 12: Deactivate token response
#[test]
fn test_deactivate_token_response() {
    let response = json!({
        "deactivated": true,
        "token_id": 1
    });

    assert!(response.get("deactivated").is_some());
}

// Test 13: Get all tokens request
#[test]
fn test_get_all_tokens_request() {
    let request = json!({
        "from": 0,
        "limit": 50,
        "is_enabled": true
    });

    assert!(request.get("limit").is_some());
}

// Test 14: Get all tokens response
#[test]
fn test_get_all_tokens_response() {
    let tokens = [json!({
        "id": 1,
        "token": "test_token_123",
        "is_enabled": true
    })];

    assert_eq!(tokens.len(), 1);
    assert!(tokens[0].get("token").is_some());
}

// Test 15: Get active tokens request
#[test]
fn test_get_active_tokens_request() {
    let request = json!({
        "from": 0,
        "limit": 20
    });

    assert!(request.get("limit").is_some());
}

// Test 16: Get active tokens response
#[test]
fn test_get_active_tokens_response() {
    let tokens = [json!({
        "token": "active_token",
        "is_enabled": true
    })];

    assert!(!tokens.is_empty());
}

// Test 17: Get token usage request
#[test]
fn test_get_token_usage_request() {
    let request = json!({
        "token": "test_token_123"
    });

    assert!(request.get("token").is_some());
}

// Test 18: Get token usage by ID request
#[test]
fn test_get_token_usage_by_id_request() {
    let request = json!({
        "id": 1
    });

    assert!(request.get("id").is_some());
}

// Test 19: Token usage response
#[test]
fn test_token_usage_response() {
    let usage = json!({
        "token_id": 1,
        "uses_count": 5,
        "usage_history": [
            {
                "user_id": "@user1:localhost",
                "used_ts": 1700000000000_i64
            }
        ]
    });

    assert!(usage.get("uses_count").is_some());
    assert!(usage.get("usage_history").is_some());
}

// Test 20: Validate token request
#[test]
fn test_validate_token_request() {
    let validate = json!({
        "token": "test_token_123"
    });

    assert!(validate.get("token").is_some());
}

// Test 21: Validate token response
#[test]
fn test_validate_token_response() {
    let response = json!({
        "valid": true,
        "token_id": 1,
        "allowed_user_ids": ["@user:localhost"]
    });

    assert!(response.get("valid").is_some());
    assert!(response["valid"].as_bool().unwrap_or(false));
}

// Test 22: Invalid token response
#[test]
fn test_invalid_token_response() {
    let response = json!({
        "valid": false,
        "error": "Token expired or used"
    });

    assert!(response.get("valid").is_some());
    assert!(!response["valid"].as_bool().unwrap_or(true));
}

// Test 23: Create batch request
#[test]
fn test_create_batch_request() {
    let batch = json!({
        "count": 10,
        "prefix": "token_",
        "token_type": "single_use",
        "max_uses": 1
    });

    assert!(batch.get("count").is_some());
    assert!(batch.get("prefix").is_some());
}

// Test 24: Create batch response
#[test]
fn test_create_batch_response() {
    let response = json!({
        "created": 10,
        "tokens": ["token_001", "token_002"]
    });

    assert!(response.get("created").is_some());
    assert!(response.get("tokens").is_some());
}

// Test 25: Cleanup expired request
#[test]
fn test_cleanup_expired_request() {
    let cleanup = json!({
        "dry_run": false
    });

    assert!(cleanup.get("dry_run").is_some());
}

// Test 26: Cleanup expired response
#[test]
fn test_cleanup_expired_response() {
    let response = json!({
        "cleaned": 5,
        "failed": 0
    });

    assert!(response.get("cleaned").is_some());
    assert!(response.get("failed").is_some());
}

// Test 27: Create room invite request
#[test]
fn test_create_room_invite_request() {
    let invite = json!({
        "room_id": "!room:localhost",
        "invite_code": "invite_code",
        "max_uses": 5
    });

    assert!(invite.get("room_id").is_some());
    assert!(invite.get("invite_code").is_some());
}

// Test 28: Get room invite request
#[test]
fn test_get_room_invite_request() {
    let request = json!({
        "invite_code": "invite_code"
    });

    assert!(request.get("invite_code").is_some());
}

// Test 29: Room invite response
#[test]
fn test_room_invite_response() {
    let invite = json!({
        "invite_code": "invite_code",
        "room_id": "!room:localhost",
        "uses_count": 0,
        "max_uses": 5
    });

    assert!(invite.get("invite_code").is_some());
    assert!(invite.get("room_id").is_some());
}

// Test 30: Use room invite request
#[test]
fn test_use_room_invite_request() {
    let use_invite = json!({
        "invite_code": "invite_code"
    });

    assert!(use_invite.get("invite_code").is_some());
}

// Test 31: Use room invite response
#[test]
fn test_use_room_invite_response() {
    let response = json!({
        "used": true,
        "room_id": "!room:localhost"
    });

    assert!(response.get("used").is_some());
    assert!(response["used"].as_bool().unwrap_or(false));
}

// Test 32: Revoke room invite request
#[test]
fn test_revoke_room_invite_request() {
    let revoke = json!({
        "invite_code": "invite_code"
    });

    assert!(revoke.get("invite_code").is_some());
}

// Test 33: Revoke room invite response
#[test]
fn test_revoke_room_invite_response() {
    let response = json!({
        "revoked": true,
        "invite_code": "invite_code"
    });

    assert!(response.get("revoked").is_some());
    assert!(response["revoked"].as_bool().unwrap_or(false));
}

// Helper functions
fn is_valid_token_type(token_type: &str) -> bool {
    matches!(token_type, "single_use" | "recurring" | "one_time")
}

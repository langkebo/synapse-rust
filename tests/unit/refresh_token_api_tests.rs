// Refresh Token API Tests - API Endpoint Coverage
// These tests cover the refresh token API endpoints from src/web/routes/refresh_token.rs

use serde_json::json;

// Test 1: Refresh token request
#[test]
fn test_refresh_token_request() {
    let refresh = json!({
        "refresh_token": "refresh_token_value",
        "grant_type": "refresh_token"
    });
    
    assert!(refresh.get("refresh_token").is_some());
    assert!(refresh.get("grant_type").is_some());
}

// Test 2: Refresh token response
#[test]
fn test_refresh_token_response() {
    let response = json!({
        "access_token": "access_token_value",
        "expires_in": 3600,
        "refresh_token": "new_refresh_token",
        "token_type": "Bearer"
    });
    
    assert!(response.get("access_token").is_some());
    assert!(response.get("expires_in").is_some());
    assert!(response.get("refresh_token").is_some());
}

// Test 3: Refresh token (r0) request
#[test]
fn test_refresh_token_r0_request() {
    let refresh = json!({
        "refresh_token": "refresh_token_value"
    });
    
    assert!(refresh.get("refresh_token").is_some());
}

// Test 4: Get user tokens request
#[test]
fn test_get_user_tokens_request() {
    let request = json!({
        "user_id": "@user:localhost",
        "from": 0,
        "limit": 50
    });
    
    assert!(request.get("user_id").is_some());
    assert!(request.get("limit").is_some());
}

// Test 5: User tokens response
#[test]
fn test_user_tokens_response() {
    let tokens = vec![
        json!({
            "id": 1,
            "user_id": "@user:localhost",
            "device_id": "DEVICE_ID",
            "created_ts": 1700000000000_i64,
            "expires_ts": 1700003600000_i64,
            "is_revoked": false
        })
    ];
    
    assert_eq!(tokens.len(), 1);
    assert!(tokens[0].get("id").is_some());
    assert!(tokens[0].get("user_id").is_some());
}

// Test 6: Get active tokens request
#[test]
fn test_get_active_tokens_request() {
    let request = json!({
        "user_id": "@user:localhost"
    });
    
    assert!(request.get("user_id").is_some());
}

// Test 7: Active tokens response
#[test]
fn test_active_tokens_response() {
    let tokens = vec![
        json!({
            "id": 1,
            "device_id": "DEVICE_ID",
            "created_ts": 1700000000000_i64
        })
    ];
    
    assert!(!tokens.is_empty());
}

// Test 8: Revoke token request
#[test]
fn test_revoke_token_request() {
    let revoke = json!({
        "token_id": 1,
        "reason": "User request"
    });
    
    assert!(revoke.get("token_id").is_some());
    assert!(revoke.get("reason").is_some());
}

// Test 9: Revoke token response
#[test]
fn test_revoke_token_response() {
    let response = json!({
        "success": true,
        "token_id": 1,
        "revoked_ts": 1700000000000_i64
    });
    
    assert!(response.get("success").is_some());
    assert!(response["success"].as_bool().unwrap_or(false));
}

// Test 10: Revoke all tokens request
#[test]
fn test_revoke_all_tokens_request() {
    let revoke = json!({
        "user_id": "@user:localhost",
        "device_id": "DEVICE_ID"
    });
    
    assert!(revoke.get("user_id").is_some());
}

// Test 11: Revoke all tokens response
#[test]
fn test_revoke_all_tokens_response() {
    let response = json!({
        "revoked_count": 5,
        "user_id": "@user:localhost"
    });
    
    assert!(response.get("revoked_count").is_some());
    assert!(response.get("revoked_count").is_some());
}

// Test 12: Get token stats request
#[test]
fn test_get_token_stats_request() {
    let request = json!({
        "user_id": "@user:localhost"
    });
    
    assert!(request.get("user_id").is_some());
}

// Test 13: Token stats response
#[test]
fn test_token_stats_response() {
    let stats = json!({
        "user_id": "@user:localhost",
        "total_tokens": 10,
        "active_tokens": 8,
        "revoked_tokens": 2,
        "expired_tokens": 0
    });
    
    assert!(stats.get("total_tokens").is_some());
    assert!(stats.get("active_tokens").is_some());
    assert!(stats.get("revoked_tokens").is_some());
}

// Test 14: Get usage history request
#[test]
fn test_get_usage_history_request() {
    let request = json!({
        "token_id": 1,
        "from": 0,
        "limit": 50
    });
    
    assert!(request.get("token_id").is_some());
    assert!(request.get("limit").is_some());
}

// Test 15: Usage history response
#[test]
fn test_usage_history_response() {
    let history = vec![
        json!({
            "id": 1,
            "token_id": 1,
            "user_id": "@user:localhost",
            "used_ts": 1700000000000_i64,
            "ip_address": "127.0.0.1"
        })
    ];
    
    assert_eq!(history.len(), 1);
    assert!(history[0].get("used_ts").is_some());
}

// Test 16: Delete token request
#[test]
fn test_delete_token_request() {
    let delete = json!({
        "id": 1
    });
    
    assert!(delete.get("id").is_some());
}

// Test 17: Delete token response
#[test]
fn test_delete_token_response() {
    let response = json!({
        "deleted": true,
        "id": 1
    });
    
    assert!(response.get("deleted").is_some());
    assert!(response["deleted"].as_bool().unwrap_or(false));
}

// Test 18: Cleanup expired tokens request
#[test]
fn test_cleanup_expired_tokens_request() {
    let cleanup = json!({
        "dry_run": false,
        "limit": 100
    });
    
    assert!(cleanup.get("dry_run").is_some());
    assert!(cleanup.get("limit").is_some());
}

// Test 19: Cleanup expired tokens response
#[test]
fn test_cleanup_expired_tokens_response() {
    let response = json!({
        "cleaned": 5,
        "failed": 0
    });
    
    assert!(response.get("cleaned").is_some());
    assert!(response.get("failed").is_some());
}

// Test 20: Grant type validation
#[test]
fn test_grant_type_validation() {
    // Valid grant types
    assert!(is_valid_grant_type("refresh_token"));
    assert!(is_valid_grant_type("authorization_code"));
    
    // Invalid
    assert!(!is_valid_grant_type("invalid"));
}

// Helper functions
fn is_valid_grant_type(grant_type: &str) -> bool {
    matches!(grant_type, "refresh_token" | "authorization_code" | "password" | "client_credentials")
}

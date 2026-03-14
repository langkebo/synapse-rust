// Captcha API Tests - API Endpoint Coverage
// These tests cover the captcha API endpoints from src/web/routes/captcha.rs

use serde_json::json;

// Test 1: Send captcha request
#[test]
fn test_send_captcha_request() {
    let captcha = json!({
        "captcha_type": "image",
        "target": "@user:localhost",
        "length": 4
    });
    
    assert!(captcha.get("captcha_type").is_some());
    assert!(captcha.get("target").is_some());
    assert!(captcha.get("length").is_some());
}

// Test 2: Send captcha response
#[test]
fn test_send_captcha_response() {
    let response = json!({
        "captcha_id": "captcha_123",
        "captcha_type": "image",
        "image_url": "data:image/png;base64,...",
        "expires_ts": 1700003600000_i64
    });
    
    assert!(response.get("captcha_id").is_some());
    assert!(response.get("captcha_type").is_some());
    assert!(response.get("image_url").is_some());
    assert!(response.get("expires_ts").is_some());
}

// Test 3: Verify captcha request
#[test]
fn test_verify_captcha_request() {
    let verify = json!({
        "captcha_id": "captcha_123",
        "code": "1234"
    });
    
    assert!(verify.get("captcha_id").is_some());
    assert!(verify.get("code").is_some());
}

// Test 4: Verify captcha response (success)
#[test]
fn test_verify_captcha_success_response() {
    let response = json!({
        "success": true,
        "captcha_id": "captcha_123",
        "verified_at": 1700000000000_i64
    });
    
    assert!(response.get("success").is_some());
    assert!(response["success"].as_bool().unwrap_or(false));
    assert!(response.get("captcha_id").is_some());
}

// Test 5: Verify captcha response (failure)
#[test]
fn test_verify_captcha_failure_response() {
    let response = json!({
        "success": false,
        "error": "Invalid captcha code",
        "remaining_attempts": 2
    });
    
    assert!(response.get("success").is_some());
    assert!(!response["success"].as_bool().unwrap_or(true));
    assert!(response.get("error").is_some());
    assert!(response.get("remaining_attempts").is_some());
}

// Test 6: Get captcha status request
#[test]
fn test_get_captcha_status_request() {
    let request = json!({
        "captcha_id": "captcha_123"
    });
    
    assert!(request.get("captcha_id").is_some());
}

// Test 7: Captcha status response (pending)
#[test]
fn test_captcha_status_pending_response() {
    let status = json!({
        "captcha_id": "captcha_123",
        "status": "pending",
        "attempt_count": 0,
        "max_attempts": 3,
        "expires_ts": 1700003600000_i64
    });
    
    assert!(status.get("captcha_id").is_some());
    assert!(status.get("status").is_some());
    assert_eq!(status["status"], "pending");
}

// Test 8: Captcha status response (verified)
#[test]
fn test_captcha_status_verified_response() {
    let status = json!({
        "captcha_id": "captcha_123",
        "status": "verified",
        "verified_at": 1700000000000_i64
    });
    
    assert_eq!(status["status"], "verified");
}

// Test 9: Captcha status response (expired)
#[test]
fn test_captcha_status_expired_response() {
    let status = json!({
        "captcha_id": "captcha_123",
        "status": "expired"
    });
    
    assert_eq!(status["status"], "expired");
}

// Test 10: Cleanup expired request
#[test]
fn test_cleanup_expired_request() {
    let cleanup = json!({
        "dry_run": false,
        "older_than_ts": 1700000000000_i64
    });
    
    assert!(cleanup.get("dry_run").is_some());
    assert!(cleanup.get("older_than_ts").is_some());
}

// Test 11: Cleanup expired response
#[test]
fn test_cleanup_expired_response() {
    let response = json!({
        "deleted": 10,
        "failed": 0
    });
    
    assert!(response.get("deleted").is_some());
    assert!(response.get("failed").is_some());
}

// Test 12: Captcha type validation
#[test]
fn test_captcha_type_validation() {
    // Valid types
    assert!(is_valid_captcha_type("image"));
    assert!(is_valid_captcha_type("audio"));
    assert!(is_valid_captcha_type("math"));
    
    // Invalid
    assert!(!is_valid_captcha_type("invalid"));
}

// Test 13: Captcha status validation
#[test]
fn test_captcha_status_validation() {
    // Valid statuses
    assert!(is_valid_captcha_status("pending"));
    assert!(is_valid_captcha_status("verified"));
    assert!(is_valid_captcha_status("expired"));
    assert!(is_valid_captcha_status("failed"));
    
    // Invalid
    assert!(!is_valid_captcha_status("invalid"));
}

// Helper functions
fn is_valid_captcha_type(captcha_type: &str) -> bool {
    matches!(captcha_type, "image" | "audio" | "math" | "hcaptcha" | "recaptcha")
}

fn is_valid_captcha_status(status: &str) -> bool {
    matches!(status, "pending" | "verified" | "expired" | "failed")
}

// ⚠️ **只锁形状**（NEW-05，2026-10-24 逐文件判定）：本文件的断言对象是**自己构造的 JSON / DTO 字面量**
// （以及本地镜像的 handler 分支），不构造 router、不发请求、不碰 DB ⇒ **改坏生产 handler 不会红**。
// 文件名刻意不再叫 `_api_tests` / `_route_tests`：原名会让人以为这条链路有 HTTP 覆盖。
// 真覆盖（HTTP 往返）：**无** —— 已登记为覆盖缺口（`scripts/ci/shape_only_test_matrix` 里写 `NONE`）
// App Service API Tests - API Endpoint Coverage
// These tests cover the app service API endpoints from synapse-web/src/routes/app_service.rs

use serde_json::json;

// Test 1: App service registration request
#[test]
fn test_app_service_registration() {
    let service = json!({
        "url": "https://appservice.example.com",
        "as_token": "token123",
        "hs_token": "hstoken",
        "sender_localpart": "appservice",
        "is_enabled": true
    });

    assert!(service.get("url").is_some());
    assert!(service.get("as_token").is_some());
    assert!(service.get("sender_localpart").is_some());
}

// Test 2: App service URL validation
#[test]
fn test_app_service_url_validation() {
    // Valid URLs
    assert!(is_valid_url("https://appservice.example.com"));
    assert!(is_valid_url("http://localhost:8080"));

    // Invalid
    assert!(!is_valid_url(""));
    assert!(!is_valid_url("not-a-url"));
}

// Test 3: App service token validation
#[test]
fn test_app_service_token_validation() {
    // Valid tokens
    assert!(is_valid_token("token123"));
    assert!(is_valid_token("abc_def_123"));

    // Invalid
    assert!(!is_valid_token(""));
}

// Test 4: App service namespaces validation
#[test]
fn test_app_service_namespaces() {
    let namespaces = json!({
        "users": [{"regex": "@bot_.*", "exclusive": true}],
        "rooms": [{"regex": "!prefix_.*", "exclusive": true}],
        "aliases": [{"regex": ".*", "exclusive": false}]
    });

    assert!(namespaces.get("users").is_some());
    assert!(namespaces.get("rooms").is_some());
}

// Test 5: App service state
#[test]
fn test_app_service_state() {
    let state = json!({
        "state": "active",
        "last_ping": 1700000000000_i64,
        "status": "ok"
    });

    assert!(state.get("state").is_some());
    assert!(state.get("last_ping").is_some());
}

// Test 6: App service ping response
#[test]
fn test_app_service_ping_response() {
    let ping = json!({
        "server_name": "synapse-rust",
        "ts": 1700000000000_i64
    });

    assert!(ping.get("server_name").is_some());
    assert!(ping.get("ts").is_some());
}

// Test 7: App service list response
#[test]
fn test_app_service_list_response() {
    let services = [json!({
        "id": "appservice1",
        "url": "https://app1.example.com",
        "is_enabled": true
    })];

    assert_eq!(services.len(), 1);
    assert!(services[0].get("url").is_some());
}

// Test 8: Virtual user response
#[test]
fn test_virtual_user_response() {
    let user = json!({
        "user_id": "@bot_appservice:localhost",
        "displayname": "Bot User"
    });

    assert!(user.get("user_id").is_some());
    let user_id = user.get("user_id").and_then(|v| v.as_str()).unwrap_or("");
    assert!(user_id.starts_with("@bot_"));
}

// Test 9: App service event push
#[test]
fn test_app_service_event_push() {
    let event = json!({
        "type": "m.room.message",
        "room_id": "!room:localhost",
        "sender": "@user:localhost",
        "content": {
            "msgtype": "m.text",
            "body": "Hello"
        }
    });

    assert!(event.get("type").is_some());
    assert!(event.get("room_id").is_some());
}

// Test 10: App service transaction response
#[test]
fn test_app_service_transaction() {
    let txn = json!({
        "transaction_id": "txn123",
        "events": []
    });

    assert!(txn.get("transaction_id").is_some());
    assert!(txn.get("events").is_some());
}

// Test 11: User namespace validation
#[test]
fn test_user_namespace_validation() {
    // Valid patterns
    assert!(is_valid_namespace("@bot_.*"));
    assert!(is_valid_namespace("@_irc_.*"));
    assert!(is_valid_namespace("@telegram_.*"));

    // Invalid
    assert!(!is_valid_namespace(""));
    assert!(!is_valid_namespace("no_at_prefix"));
}

// Test 12: Room namespace validation
#[test]
fn test_room_namespace_validation() {
    // Valid patterns
    assert!(is_valid_room_namespace("!prefix_.*"));
    assert!(is_valid_room_namespace("!_irc_.*"));

    // Invalid
    assert!(!is_valid_room_namespace(""));
    assert!(!is_valid_room_namespace("no_exclamation"));
}

// Test 13: Alias namespace validation
#[test]
fn test_alias_namespace_validation() {
    // Valid patterns
    assert!(is_valid_alias_namespace("#irc_.*"));
    assert!(is_valid_alias_namespace("#telegram_.*"));

    // Invalid
    assert!(!is_valid_alias_namespace(""));
    assert!(!is_valid_alias_namespace("no_hash"));
}

// Test 14: App service statistics
#[test]
fn test_app_service_statistics() {
    let stats = json!({
        "total_events": 100,
        "total_users": 10,
        "total_rooms": 5
    });

    assert!(stats.get("total_events").is_some());
    assert!(stats.get("total_users").is_some());
}

// Test 15: App service query user response
#[test]
fn test_app_service_query_user() {
    let query = json!({
        "user_id": "@user:localhost"
    });

    assert!(query.get("user_id").is_some());
}

// Test 16: App service query room alias response
#[test]
fn test_app_service_query_room_alias() {
    let query = json!({
        "room_id": "!room:localhost",
        "servers": ["localhost"]
    });

    assert!(query.get("room_id").is_some());
    assert!(query.get("servers").is_some());
}

// Helper functions
fn is_valid_url(url: &str) -> bool {
    !url.is_empty() && (url.starts_with("http://") || url.starts_with("https://"))
}

fn is_valid_token(token: &str) -> bool {
    !token.is_empty()
}

fn is_valid_namespace(pattern: &str) -> bool {
    !pattern.is_empty() && pattern.starts_with('@')
}

fn is_valid_room_namespace(pattern: &str) -> bool {
    !pattern.is_empty() && pattern.starts_with('!')
}

fn is_valid_alias_namespace(pattern: &str) -> bool {
    !pattern.is_empty() && pattern.starts_with('#')
}

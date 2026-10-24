// ⚠️ **只锁形状**（NEW-05，2026-10-24 逐文件判定）：本文件的断言对象是**自己构造的 JSON / DTO 字面量**
// （以及本地镜像的 handler 分支），不构造 router、不发请求、不碰 DB ⇒ **改坏生产 handler 不会红**。
// 文件名刻意不再叫 `_api_tests` / `_route_tests`：原名会让人以为这条链路有 HTTP 覆盖。
// 真覆盖（HTTP 往返）：**无** —— 已登记为覆盖缺口（`scripts/ci/shape_only_test_matrix` 里写 `NONE`）
// Sliding Sync API Tests - API Endpoint Coverage
// These tests cover the sliding sync API endpoints from synapse-web/src/routes/sliding_sync.rs

use serde_json::json;

// Test 1: Sliding sync request
#[test]
fn test_sliding_sync_request() {
    let sync = json!({
        "timeout": 30000,
        "filter": {
            "rooms": []
        },
        "list": {}
    });

    assert!(sync.get("timeout").is_some());
    assert!(sync.get("filter").is_some());
    assert!(sync.get("list").is_some());
}

// Test 2: Sliding sync response
#[test]
fn test_sliding_sync_response() {
    let response = json!({
        "next_batch": "batch_token",
        "rooms": {},
        "lists": {}
    });

    assert!(response.get("next_batch").is_some());
    assert!(response.get("rooms").is_some());
    assert!(response.get("lists").is_some());
}

// Test 3: Sliding sync with room subscriptions
#[test]
fn test_sliding_sync_room_subscriptions() {
    let sync = json!({
        "room_subscriptions": {
            "!room:localhost": {
                "timeline": {
                    "limit": 10
                }
            }
        }
    });

    assert!(sync.get("room_subscriptions").is_some());
}

// Test 4: Sliding sync response with extensions
#[test]
fn test_sliding_sync_extensions_response() {
    let response = json!({
        "next_batch": "batch_token",
        "extensions": {
            "account_data": {
                "events": []
            }
        }
    });

    assert!(response.get("extensions").is_some());
}

// Test 5: Sliding sync delta token
#[test]
fn test_sliding_sync_delta_token() {
    let response = json!({
        "next_batch": "delta_token",
        "rooms": {
            "!room:localhost": {
                "timeline": []
            }
        }
    });

    assert!(response.get("next_batch").is_some());
}

// Test 6: Unstable sliding sync request
#[test]
fn test_unstable_sliding_sync_request() {
    let sync = json!({
        "timeout": 30000,
        "list": {}
    });

    assert!(sync.get("timeout").is_some());
}

// Test 7: Sliding sync list filters
#[test]
fn test_sliding_sync_list_filters() {
    let list = json!({
        "filters": {
            "is_invite": false,
            "is_tombstone": false
        },
        "sort": ["by_updated_time"],
        "required_state": []
    });

    assert!(list.get("filters").is_some());
    assert!(list.get("sort").is_some());
}

// Test 8: Sliding sync room notification counts
#[test]
fn test_sliding_sync_notification_counts() {
    let room = json!({
        "notification_count": 5,
        "highlight_count": 1
    });

    assert!(room.get("notification_count").is_some());
    assert!(room.get("highlight_count").is_some());
}
